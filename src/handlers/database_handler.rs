use axum::
{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::json;
use crate::
{
    error::AppError,
    services::{database_service, jwt::Claims, project_service},
    state::AppState,
};

pub async fn create_database_handler(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError>
{
    let (db_record, password) = database_service::provision_database(
        &state.db_pool,
        &state.mariadb_pool,
        &claims.sub,
        &state.config.encryption_key,
    ).await?;

    let response = json!({
        "message": "Database created successfully.",
        "database": {
            "id": db_record.id,
            "database_name": db_record.database_name,
            "username": db_record.username,
            "password": password,
            "host": state.config.mariadb_public_host,
            "port": state.config.mariadb_public_port,
        }
    });

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn get_my_database_handler(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError>
{
    match database_service::get_database_by_owner(&state.db_pool, &claims.sub).await?
    {
        Some(db) =>
        {
            let details = database_service::create_db_details_response(db, &state.config, &state.config.encryption_key)?;
            Ok(Json(json!({ "database": details })))
        }
        None => Err(AppError::NotFound("No database found for the current user.".to_string())),
    }
}

pub async fn delete_my_database_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(db_id): Path<i32>,
) -> Result<impl IntoResponse, AppError>
{
    database_service::deprovision_database(
        &state.db_pool,
        &state.mariadb_pool,
        db_id,
        &claims.sub,
    ).await?;

    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Database deleted successfully."}))))
}

pub async fn link_database_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path((project_id, db_id)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError>
{
    let project = project_service::get_project_by_id_and_owner(
        &state.db_pool, project_id, &claims.sub, false
    ).await?.ok_or(AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    let database = database_service::get_database_by_id_and_owner(
        &state.db_pool, db_id, &claims.sub
    ).await?.ok_or(AppError::NotFound("Database not found or you are not the owner.".to_string()))?;

    database_service::link_database_to_project(&state.db_pool, database.id, project.id, &claims.sub).await?;

    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Database linked to project successfully."}))))
}

pub async fn unlink_database_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError>
{
    project_service::get_project_by_id_and_owner(&state.db_pool, project_id, &claims.sub, false).await?
    .ok_or(AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    database_service::unlink_database_from_project(&state.db_pool, project_id, &claims.sub).await?;
    
    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Database unlinked from project successfully."}))))
}