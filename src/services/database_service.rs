use crate::
{
    config::Config,
    error::{AppError, DatabaseErrorCode, ProjectErrorCode},
    model::database::{Database, DatabaseDetailsResponse},
    services::crypto_service,
};
use rand::distr::{Alphanumeric, SampleString};
use sqlx::{MySqlPool, PgPool, Postgres, Transaction};
use tracing::{error, info, warn};
use base64::prelude::*;
use std::collections::HashSet;

const DB_PREFIX: &str = "hangardb";


fn valid_identifier(s: &str) -> bool 
{
    if s.is_empty() 
    {
        return false;
    }
    let allowed_chars: HashSet<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_".chars().collect();
    s.chars().all(|c| allowed_chars.contains(&c))
}

pub async fn check_database_exists_for_owner(pool: &PgPool, owner: &str) -> Result<bool, AppError>
{
    let count: (i64, ) = sqlx::query_as("SELECT COUNT(*) FROM databases WHERE owner_login = $1")
        .bind(owner)
        .fetch_one(pool)
        .await
        .map_err(|e|
        {
            error!("Failed to check if database exists for owner {}: {}", owner, e);
            AppError::InternalServerError
        })?;
    Ok(count.0 > 0)
}

fn generate_password() -> String
{
    let password = Alphanumeric.sample_string(&mut rand::rng(), 24);
    password
}

pub async fn provision_database(
    pg_pool: &PgPool,
    mariadb_pool: &MySqlPool,
    owner_login: &str,
    encryption_key: &[u8],
) -> Result<(Database, String), AppError>
{
    if check_database_exists_for_owner(pg_pool, owner_login).await?
    {
        return Err(DatabaseErrorCode::DatabaseAlreadyExists.into());
    }

    let db_name = format!("{}_{}", DB_PREFIX, owner_login);
    let username = db_name.clone();
    let password = generate_password();

    if let Err(e) = execute_mariadb_provisioning(mariadb_pool, &db_name, &username, &password).await
    {
        warn!("MariaDB provisioning failed for user '{}'. Attempting rollback. Error: {}", owner_login, e);
        if let Err(e) = execute_mariadb_deprovisioning(mariadb_pool, &db_name, &username).await
        {
            error!("Failed to rollback MariaDB provisioning for user '{}': {}", owner_login, e);
        }
        return Err(e);
    }

    let encrypted_password_vec = crypto_service::encrypt(&password, encryption_key)?;
    let encrypted_password = BASE64_STANDARD.encode(encrypted_password_vec);

    let db_record = sqlx::query_as::<_, Database>(
        "INSERT INTO databases (owner_login, database_name, username, encrypted_password)
         VALUES ($1, $2, $3, $4)
         RETURNING id, owner_login, database_name, username, encrypted_password, project_id, created_at",
    )
    .bind(owner_login)
    .bind(&db_name)
    .bind(&username)
    .bind(&encrypted_password)
    .fetch_one(pg_pool)
    .await
    .map_err(|e|
    {
        error!("Failed to persist database metadata for user '{}' after successful MariaDB provisioning: {}", owner_login, e);
        let mariadb_pool = mariadb_pool.clone();
        let db_name = db_name.clone();
        let username = username.clone();
        let owner_login = owner_login.to_string();
        tokio::spawn(async move
        {
            warn!("CRITICAL: Rolling back MariaDB provisioning for {} due to PostgreSQL failure.", owner_login);
            if let Err(e) = execute_mariadb_deprovisioning(&mariadb_pool, &db_name, &username).await
            {
                error!("Failed to rollback MariaDB provisioning for user '{}': {}", owner_login, e);
            }
        });
        AppError::InternalServerError
    })?;

    info!("Database for user '{}' provisioned successfully.", owner_login);
    Ok((db_record, password))
}

pub async fn deprovision_database(
    pg_pool: &PgPool,
    mariadb_pool: &MySqlPool,
    db_id: i32,
    owner_login: &str,
) -> Result<(), AppError>
{
    let db_record = get_database_by_id_and_owner(pg_pool, db_id, owner_login).await?
        .ok_or(DatabaseErrorCode::NotFound)?;

    execute_mariadb_deprovisioning(mariadb_pool, &db_record.database_name, &db_record.username).await?;

    sqlx::query("DELETE FROM databases WHERE id = $1")
        .bind(db_id)
        .execute(pg_pool)
        .await
        .map_err(|e|
        {
            error!("Failed to delete database metadata for ID {}: {}", db_id, e);
            AppError::InternalServerError // La DB a été supprimée mais pas la métadonnée.
        })?;

    info!("Database ID {} for user '{}' deprovisioned successfully.", db_id, owner_login);
    Ok(())
}

async fn execute_mariadb_provisioning(
    pool: &MySqlPool,
    db_name: &str,
    username: &str,
    password: &str,
) -> Result<(), AppError> 
{
    if !valid_identifier(db_name) || !valid_identifier(username) 
    {
        return Err(AppError::BadRequest("Invalid identifier".into()));
    }

    let mut conn = pool.acquire().await.map_err(|_| DatabaseErrorCode::ProvisioningFailed)?;

    sqlx::query(&format!(
        "CREATE DATABASE `{}` CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci",
        db_name
    ))
    .execute(&mut *conn)
    .await
    .map_err(|_| DatabaseErrorCode::ProvisioningFailed)?;

    let create_user_sql = format!("CREATE USER `{}`@'%' IDENTIFIED BY ?", username);
    sqlx::query(&create_user_sql)
        .bind(password)
        .execute(&mut *conn)
        .await
        .map_err(|_| DatabaseErrorCode::ProvisioningFailed)?;

    let grant_sql = format!("GRANT ALL PRIVILEGES ON `{}`.* TO `{}`@'%'", db_name, username);
    sqlx::query(&grant_sql)
        .execute(&mut *conn)
        .await
        .map_err(|_| DatabaseErrorCode::ProvisioningFailed)?;

    Ok(())
}

async fn execute_mariadb_deprovisioning(
    pool: &MySqlPool,
    db_name: &str,
    username: &str,
) -> Result<(), AppError>
{
    if !valid_identifier(db_name) || !valid_identifier(username) 
    {
        return Err(AppError::BadRequest("Invalid identifier".into()));
    }

    let mut conn = pool.acquire().await.map_err(|_| DatabaseErrorCode::DeprovisioningFailed)?;
    sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", db_name))
        .execute(&mut *conn)
        .await
        .map_err(|_| DatabaseErrorCode::DeprovisioningFailed)?;
    sqlx::query(&format!("DROP USER IF EXISTS `{}`@'%'", username))
        .execute(&mut *conn)
        .await
        .map_err(|_| DatabaseErrorCode::DeprovisioningFailed)?;
    Ok(())
}

pub async fn get_database_by_owner(pool: &PgPool, owner: &str) -> Result<Option<Database>, AppError>
{
    sqlx::query_as("SELECT * FROM databases WHERE owner_login = $1")
        .bind(owner)
        .fetch_optional(pool)
        .await
        .map_err(|e|
        {
            error!("Failed to fetch database for owner {}: {}", owner, e);
            AppError::InternalServerError
        })
}

pub async fn get_database_by_id_and_owner(pool: &PgPool, db_id: i32, owner: &str) -> Result<Option<Database>, AppError>
{
    sqlx::query_as("SELECT * FROM databases WHERE id = $1 AND owner_login = $2")
        .bind(db_id)
        .bind(owner)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::InternalServerError)
}

pub async fn get_database_by_project_id(pool: &PgPool, project_id: i32) -> Result<Option<Database>, AppError>
{
    sqlx::query_as("SELECT * FROM databases WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| AppError::InternalServerError)
}

pub async fn link_database_to_project(pool: &PgPool, db_id: i32, project_id: i32, owner: &str) -> Result<(), AppError>
{
    let result = sqlx::query("UPDATE databases SET project_id = $1 WHERE id = $2 AND owner_login = $3")
        .bind(project_id)
        .bind(db_id)
        .bind(owner)
        .execute(pool)
        .await
        .map_err(|_| AppError::InternalServerError)?;
    
    if result.rows_affected() == 0 {
        return Err(DatabaseErrorCode::NotFound.into());
    }
    Ok(())
}

pub async fn unlink_database_from_project(pool: &PgPool, project_id: i32, owner: &str) -> Result<(), AppError>
{
    let result = sqlx::query("UPDATE databases SET project_id = NULL WHERE project_id = $1 AND owner_login = $2")
        .bind(project_id)
        .bind(owner)
        .execute(pool)
        .await
        .map_err(|_| AppError::InternalServerError)?;
        
    if result.rows_affected() == 0 {
        return Err(DatabaseErrorCode::NotFound.into());
    }
    Ok(())
}

pub async fn provision_and_link_database_tx<'a>(
    tx: &mut Transaction<'a, Postgres>,
    mariadb_pool: &MySqlPool,
    owner_login: &str,
    project_id: i32,
    encryption_key: &[u8],
) -> Result<(), AppError>
{

    let db_name = format!("{}_{}", DB_PREFIX, owner_login);
    let username = db_name.clone();
    let password = generate_password();

    if let Err(e) = execute_mariadb_provisioning(mariadb_pool, &db_name, &username, &password).await
    {
        warn!("MariaDB provisioning failed during transaction for user '{}'. Error: {}", owner_login, e);
        execute_mariadb_deprovisioning(mariadb_pool, &db_name, &username).await.ok();
        return Err(e);
    }
    
    let encrypted_password_vec = crypto_service::encrypt(&password, encryption_key)?;
    let encrypted_password = BASE64_STANDARD.encode(encrypted_password_vec);

    sqlx::query(
        "INSERT INTO databases (owner_login, database_name, username, encrypted_password, project_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(owner_login)
    .bind(&db_name)
    .bind(&username)
    .bind(&encrypted_password)
    .bind(project_id)
    .execute(&mut **tx)
    .await
    .map_err(|e|
    {
        error!("Failed to persist database metadata for user '{}' in transaction: {}", owner_login, e);
        AppError::ProjectError(ProjectErrorCode::ProjectCreationFailedWithDatabaseError)
    })?;

    Ok(())
}

pub fn create_db_details_response(db: Database, config: &Config, encryption_key: &[u8]) -> Result<DatabaseDetailsResponse, AppError>
{
    let encrypted_pass_vec = BASE64_STANDARD.decode(&db.encrypted_password).map_err(|_| AppError::InternalServerError)?;
    let password = crypto_service::decrypt(&encrypted_pass_vec, encryption_key)?;

    Ok(DatabaseDetailsResponse 
    {
        id: db.id,
        owner_login: db.owner_login,
        database_name: db.database_name,
        username: db.username,
        password,
        project_id: db.project_id,
        host: config.mariadb_public_host.clone(),
        port: config.mariadb_public_port,
        created_at: db.created_at,
    })
}