use sqlx::{PgPool, Postgres, Transaction};
use crate::{error::AppError, model::project::Project};

pub async fn check_project_name_exists(pool: &PgPool, name: &str) -> Result<bool, AppError> 
{
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE name = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::InternalServerError)?;
    Ok(count.0 > 0)
}

pub async fn check_owner_exists(pool: &PgPool, owner: &str) -> Result<bool, AppError> 
{
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE owner = $1")
        .bind(owner)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::InternalServerError)?;
    Ok(count.0 > 0)
}

pub async fn create_project<'a>(
    tx: &mut Transaction<'a, Postgres>,
    name: &str,
    owner: &str,
    image_url: &str,
    container_id: &str,
) -> Result<Project, AppError> 
{
    let project = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (name, owner, image_url, container_id) VALUES ($1, $2, $3, $4) RETURNING *"
    )
        .bind(name)
        .bind(owner)
        .bind(image_url)
        .bind(container_id)
        .fetch_one(&mut **tx) 
        .await
        .map_err(|e: sqlx::Error| 
        {
            if let Some(db_err) = e.as_database_error() 
            {
                if db_err.is_unique_violation() 
                {
                    return AppError::BadRequest("Project name or owner already exists.".to_string());
                }
            }
            AppError::InternalServerError
        })?;

    Ok(project)
}