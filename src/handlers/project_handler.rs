use std::collections::HashSet;

use axum::{extract::{Path, State}, http::StatusCode, response::{IntoResponse, Json}};
use bollard::Docker;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, error, info, warn};
use crate::
{
    error::{AppError, ProjectErrorCode}, model::project::{ProjectDetailsResponse, ProjectMetrics}, services::{docker_service, jwt::Claims, project_service, validation_service}, state::AppState
};

#[derive(Deserialize)]
pub struct DeployPayload 
{
    project_name: String,
    image_url: String,
    participants: Vec<String>,
}

#[derive(Clone, Copy)]
enum ProjectAction 
{
    Start,
    Stop,
    Restart,
}

#[derive(Deserialize)]
pub struct UpdateImagePayload 
{
    new_image_url: String,
}

#[derive(Deserialize)]
pub struct ParticipantPayload 
{
    participant_id: String,
}


impl ProjectAction 
{
    async fn execute(self, docker: Docker, container_name: String) -> Result<(), AppError> 
    {
        match self 
        {
            ProjectAction::Start   => docker_service::start_container_by_name(&docker, &container_name).await,
            ProjectAction::Stop    => docker_service::stop_container_by_name(&docker, &container_name).await,
            ProjectAction::Restart => docker_service::restart_container_by_name(&docker, &container_name).await,
        }
    }
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

    let participants: HashSet<String> = payload.participants.into_iter().collect();
    if participants.contains(&user_login) 
    {
        return Err(ProjectErrorCode::OwnerCannotBeParticipant.into());
    }
    let final_participants: Vec<String> = participants.into_iter().collect();

    if project_service::check_owner_exists(&state.db_pool, &user_login).await?
    {
        return Err(ProjectErrorCode::OwnerAlreadyExists.into());
    }
    if project_service::check_project_name_exists(&state.db_pool, &payload.project_name).await?
    {
        return Err(ProjectErrorCode::ProjectNameTaken.into());
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

            tx.rollback().await.map_err(|_| AppError::InternalServerError)?;
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

    if let Err(e) = project_service::add_project_participants(&mut tx, new_project.id, &final_participants).await
    {
        warn!("Failed to add participants, rolling back container and image...");
        tx.rollback().await.map_err(|_| AppError::InternalServerError)?;

        let docker_client = state.docker_client.clone();
        let image_url = payload.image_url.clone();
        let container_id = container_id.clone();

        tokio::spawn(async move 
        {
            if let Err(e) = docker_service::remove_container(&docker_client, &container_id).await 
            {
                error!("Participant Rollback: Failed to remove container {}: {}", container_id, e);
            }
            if let Err(e) = docker_service::remove_image(&docker_client, &image_url).await
            {
                error!("Participant Rollback: Failed to remove image {}: {}", image_url, e);
            }
        });
        return Err(e);
    }

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

pub async fn get_project_details_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = claims.sub;
    debug!("User '{}' fetching details for project ID: {}", user_login, project_id);

    match project_service::get_project_by_id_for_user(&state.db_pool, project_id, &user_login).await? 
    {
        Some(project) => 
        {
            let participants = project_service::get_project_participants(&state.db_pool, project.id).await?;

            let response = ProjectDetailsResponse 
            {
                project,
                participants,
            };

            Ok((
                StatusCode::OK,
                Json(json!({ "project": response })),
            ))
        },
        None => 
        {
            warn!("Access denied or not found for project ID {} by user '{}'.", project_id, user_login);
            Err(AppError::NotFound(format!("Project with ID {} not found or you don't have access.", project_id)))
        }
    }
}

pub async fn get_project_status_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    let project = project_service::get_project_by_id_for_user(&state.db_pool, project_id, &claims.sub).await?
        .ok_or_else(|| AppError::NotFound("Project not found or access denied.".to_string()))?;

    let container_name = format!("{}-{}", &state.config.app_prefix, project.name);
    let status = docker_service::get_container_status(&state.docker_client, &container_name).await?;

    Ok(Json(json!({ "status": status.and_then(|s| s.status) })))
}

async fn project_control_handler(
    state: AppState,
    claims: Claims,
    project_id: i32,
    action: ProjectAction,
) -> Result<impl IntoResponse, AppError> 
{
    let project = project_service::get_project_by_id_and_owner(&state.db_pool,project_id, &claims.sub).await?
    .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    let container_name = format!("{}-{}", &state.config.app_prefix, project.name);

    action.execute(state.docker_client.clone(), container_name).await?;

    Ok(StatusCode::OK)
}

pub async fn start_project_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    project_control_handler(state, claims, project_id, ProjectAction::Start).await
}

pub async fn stop_project_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    project_control_handler(state, claims, project_id, ProjectAction::Stop).await
}

pub async fn restart_project_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    project_control_handler(state, claims, project_id, ProjectAction::Restart).await
}

pub async fn get_project_logs_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> 
{
    let project = project_service::get_project_by_id_for_user(&state.db_pool, project_id, &claims.sub).await?
        .ok_or_else(|| AppError::NotFound("Project not found or access denied.".to_string()))?;

    let container_name = format!("{}-{}", &state.config.app_prefix, project.name);

    let logs = docker_service::get_container_logs(&state.docker_client, &container_name, "200").await?;

    Ok(Json(json!({ "logs": logs })))
}

pub async fn get_project_metrics_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<Json<ProjectMetrics>, AppError> 
{
    let project = project_service::get_project_by_id_for_user(&state.db_pool, project_id, &claims.sub).await?
        .ok_or_else(|| AppError::NotFound("Project not found or access denied.".to_string()))?;

    let container_name = format!("{}-{}", &state.config.app_prefix, project.name);

    debug!("Fetching metrics for container '{}' (Project ID: {})", container_name, project_id);
    
    let metrics = docker_service::get_container_metrics(&state.docker_client, &container_name).await?;

    Ok(Json(metrics))
}

pub async fn update_project_image_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
    Json(payload): Json<UpdateImagePayload>,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = &claims.sub;
    info!("User '{}' initiated image update for project ID: {}", user_login, project_id);

    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login)
        .await?
        .ok_or_else(|| 
        {
            warn!("Update failed: Project ID {} not found or not owned by user '{}'.", project_id, user_login);
            AppError::NotFound(format!("Project with ID {} not found or you are not the owner.", project_id))
        })?;

    validation_service::validate_image_url(&payload.new_image_url)?;

    docker_service::pull_image(&state.docker_client, &payload.new_image_url).await?;

    if let Err(scan_error) = docker_service::scan_image_with_grype(&payload.new_image_url, &state.config).await 
    {
        warn!("Image scan failed for '{}'. Rolling back by removing the pulled image.", payload.new_image_url);
        if let Err(e) = docker_service::remove_image(&state.docker_client, &payload.new_image_url).await 
        {
            error!("ROLLBACK FAILED: Could not remove image '{}' after scan failure: {}", payload.new_image_url, e);
        }
        return Err(scan_error);
    }

    let old_container_name = format!("{}-{}", &state.config.app_prefix, &project.name);
    docker_service::remove_container(&state.docker_client, &old_container_name).await?;

    let new_container_id = match docker_service::create_project_container(
        &state.docker_client,
        &project.name,
        &payload.new_image_url,
        &state.config,
    ).await 
    {
        Ok(id) => id,
        Err(creation_error) => 
        {
            error!("Failed to create new container for project '{}'. The service is now down.", project.name);
            if let Err(e) = docker_service::remove_image(&state.docker_client, &payload.new_image_url).await 
            {
                error!("Could not remove new image '{}' after container creation failure: {}", payload.new_image_url, e);
            }
            return Err(creation_error);
        }
    };

    project_service::update_project_image_and_container(
        &state.db_pool,
        project.id,
        &payload.new_image_url,
        &new_container_id,
    ).await?;
    
    let docker_client = state.docker_client.clone();
    let old_image_url = project.image_url.clone();
    tokio::spawn(async move
    {
        info!("Attempting to remove old image in background: {}", old_image_url);
        if let Err(e) = docker_service::remove_image(&docker_client, &old_image_url).await 
        {
             warn!("Could not remove old image '{}': {}", old_image_url, e);
        }
    });

    info!("Project '{}' image updated successfully by user '{}'.", project.name, user_login);

    Ok
    (
        (
            StatusCode::OK,
            Json
            (
                json!
                (
                    {
                        "status": "success",
                        "message": "Project image updated successfully."
                    }
                ),
            )
        )
    )
}

pub async fn add_participant_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
    Json(payload): Json<ParticipantPayload>,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = &claims.sub;
    info!("User '{}' trying to add participant '{}' to project {}", user_login, payload.participant_id, project_id);

    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    if project.owner == payload.participant_id 
    {
        return Err(ProjectErrorCode::OwnerCannotBeParticipant.into());
    }

    project_service::add_participant_to_project(&state.db_pool, project_id, &payload.participant_id).await?;

    info!("Participant '{}' added successfully to project {}", payload.participant_id, project_id);
    Ok((StatusCode::CREATED, Json(json!({"status": "success", "message": "Participant added."}))))
}

pub async fn remove_participant_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path((project_id, participant_id)): Path<(i32, String)>,
) -> Result<impl IntoResponse, AppError> 
{
    let user_login = &claims.sub;
    info!("User '{}' trying to remove participant '{}' from project {}", user_login, participant_id, project_id);

    project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    project_service::remove_participant_from_project(&state.db_pool, project_id, &participant_id).await?;
    
    info!("Participant '{}' removed successfully from project {}", participant_id, project_id);
    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Participant removed."}))))
}