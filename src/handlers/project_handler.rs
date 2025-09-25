use axum::{extract::{Path, State}, http::StatusCode, response::{IntoResponse, Json}};
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info, warn};
use crate::
{
    error::AppError, 
    services::{docker_service, jwt::Claims, project_service, validation_service}, 
    state::AppState
};

#[derive(Deserialize)]
pub struct DeployPayload 
{
    project_name: String,
    image_url: String,
}

pub async fn deploy_project_handler(
    State(state): State<AppState>,
    claims: Claims,
    Json(payload): Json<DeployPayload>,
) -> Result<impl IntoResponse, AppError> 
{
    validation_service::validate_project_name(&payload.project_name)?;
    validation_service::validate_image_url(&payload.image_url)?;

    let user_login = claims.sub;

    if project_service::check_owner_exists(&state.db_pool, &user_login).await? 
    {
        return Err(AppError::BadRequest("You already own a project. Only one project per user is allowed.".to_string()));
    }
    if project_service::check_project_name_exists(&state.db_pool, &payload.project_name).await? 
    {
        return Err(AppError::BadRequest(format!("Project name '{}' is already taken.", payload.project_name)));
    }

    docker_service::pull_image(&state.docker_client, &payload.image_url).await?;

    if let Err(scan_error) = docker_service::scan_image_with_grype(&payload.image_url, &state.config).await 
    {
        warn!("Image scan failed. Rolling back by removing the pulled image.");
        if let Err(e) = docker_service::remove_image(&state.docker_client, &payload.image_url).await
        {
            error!("Failed to remove image after image scan failure: {}", e);
        }
        return Err(scan_error);
    }

    let container_id = match docker_service::create_project_container(
        &state.docker_client,
        &payload.project_name,
        &payload.image_url,
        &state.config,
    ).await 
    {
        Ok(id) => id,
        Err(creation_error) => 
        {
            warn!("Container creation failed. Rolling back by removing the pulled image.");
            if let Err(e) = docker_service::remove_image(&state.docker_client, &payload.image_url).await
            {
                error!("Failed to remove image after container creation failure: {}", e);
            }
            return Err(creation_error);
        }
    };

    let mut tx = state.db_pool.begin().await.map_err(|_| AppError::InternalServerError)?;
    
    let new_project = match project_service::create_project(
        &mut tx,
        &payload.project_name,
        &user_login,
        &payload.image_url,
        &container_id,
    ).await 
    {
        Ok(project) => project,
        Err(db_error) => 
        {
            warn!("Failed to create project in DB, rolling back container and image...");

            let docker_client = state.docker_client.clone();
            let image_url = payload.image_url.clone();

            tokio::spawn(async move 
            {
                if let Err(e) = docker_service::remove_container(&docker_client, &container_id).await 
                {
                    error!("DB Rollback: Failed to remove container {}: {}", container_id, e);
                }
                if let Err(e) = docker_service::remove_image(&docker_client, &image_url).await
                {
                    error!("DB Rollback: Failed to remove image {}: {}", image_url, e);
                }
            });
            
            return Err(db_error);
        }
    };

    tx.commit().await.map_err(|_| AppError::InternalServerError)?;
    
    info!("Project '{}' by user '{}' created successfully.", payload.project_name, user_login);

    Ok((
        StatusCode::CREATED,
        Json
        (
            json!
            (
                {
                    "status": "success",
                    "message": "Project deployed successfully.",
                    "project": new_project
                }
            ),
        )
    ))
}

pub async fn purge_project_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = claims.sub;
    info!("User '{}' initiated purge for project ID: {}", user_login, project_id);

    let project = match project_service::get_project_by_id_and_owner(&state.db_pool, project_id, &user_login).await? 
    {
        Some(p) => p,
        None => 
        {
            warn!("Purge failed: Project ID {} not found or not owned by user '{}'.", project_id, user_login);
            return Err(AppError::NotFound(format!("Project with ID {} not found.", project_id)));
        }
    };
    
    info!("Ownership confirmed. Proceeding with purge for project '{}' (ID: {})", project.name, project.id);

    // Le reste de la logique est identique
    docker_service::remove_container(&state.docker_client, &project.container_id).await?;
    docker_service::remove_image(&state.docker_client, &project.image_url).await?;
    project_service::delete_project_by_id(&state.db_pool, project.id).await?;

    info!("Successfully purged project '{}' for user '{}'.", project.name, user_login);

    Ok(
        (
            StatusCode::OK,
            Json(json!({
                "status": "success",
                "message": "Project purged successfully."
            })),
        )
    )
}

pub async fn list_owned_projects_handler(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = claims.sub;
    info!("Fetching owned projects for user '{}'", user_login);

    let projects = project_service::get_projects_by_owner(&state.db_pool, &user_login).await?;

    Ok(
        (
            StatusCode::OK,
            Json(json!({ "projects": projects })),
        )
    )
}

pub async fn list_participating_projects_handler(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = claims.sub;
    info!("Fetching projects where user '{}' is a participant", user_login);

    let projects = project_service::get_participating_projects(&state.db_pool, &user_login).await?;

    Ok(
        (
            StatusCode::OK,
            Json(json!({ "projects": projects })),
        )
    )
}

