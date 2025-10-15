use std::{collections::{HashMap, HashSet}, fs};
use axum::
{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use serde_json::json;
use tempfile::Builder as TempBuilder;
use tracing::{debug, error, info, warn};

use crate::{error::DatabaseErrorCode, services::crypto_service};
use base64::prelude::*;

use crate::
{
    error::{AppError, ProjectErrorCode},
    model::project::{ProjectDetailsResponse, ProjectMetrics, ProjectSourceType},
    services::{docker_service, github_service, jwt::Claims, project_service, validation_service, database_service},
    state::AppState,
};

#[derive(Deserialize)]
pub struct DeployPayload
{
    project_name: String,
    image_url: Option<String>,
    github_repo_url: Option<String>,
    participants: Vec<String>,
    env_vars: Option<HashMap<String, String>>,
    persistent_volume_path: Option<String>,
    create_database: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateEnvPayload
{
    env_vars: HashMap<String, String>,
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
    async fn execute(self, docker: bollard::Docker, container_name: String) -> Result<(), AppError>
    {
        match self
        {
            ProjectAction::Start => docker_service::start_container_by_name(&docker, &container_name).await,
            ProjectAction::Stop => docker_service::stop_container_by_name(&docker, &container_name).await,
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

    if let Some(vars) = &payload.env_vars
    {
        validation_service::validate_env_vars(vars)?;
    }

    let mut persistent_volume_path = payload.persistent_volume_path.clone();
    if let Some(path) = &persistent_volume_path
    {
        validation_service::validate_volume_path(path)?;
    }

    let user_login = claims.sub;

    if project_service::check_owner_exists(&state.db_pool, &user_login).await?
    {
        return Err(ProjectErrorCode::OwnerAlreadyExists.into());
    }
    if project_service::check_project_name_exists(&state.db_pool, &payload.project_name).await?
    {
        return Err(ProjectErrorCode::ProjectNameTaken.into());
    }

    if payload.create_database.unwrap_or(false) 
    {
        if database_service::check_database_exists_for_owner(&state.db_pool, &user_login).await? 
        {
            return Err(AppError::DatabaseError(DatabaseErrorCode::DatabaseAlreadyExists));
        }
    }

    let participants: HashSet<String> = payload.participants.into_iter().collect();
    if participants.contains(&user_login)
    {
        return Err(ProjectErrorCode::OwnerCannotBeParticipant.into());
    }
    let final_participants: Vec<String> = participants.into_iter().collect();

    let (source_type, source_url, deployed_image_tag) = if let Some(image_url) = &payload.image_url
    {
        let tag = prepare_direct_source(&state, image_url).await?;
        (ProjectSourceType::Direct, image_url.clone(), tag)
    }
    else if let Some(github_repo_url) = &payload.github_repo_url
    {
        persistent_volume_path = Some("/var/www/html".to_string());
        let tag = prepare_github_source(&state, &payload.project_name, github_repo_url).await?;
        (ProjectSourceType::Github, github_repo_url.clone(), tag)
    }
    else
    {
        return Err(AppError::BadRequest("You must provide either an 'image_url' or a 'github_repo_url'.".to_string()));
    };

    let (container_name, volume_name) = match docker_service::create_project_container(
        &state.docker_client,
        &payload.project_name,
        &deployed_image_tag,
        &state.config,
        &payload.env_vars,
        &persistent_volume_path,
    ).await
    {
        Ok(name) => name,
        Err(e) =>
        {
            warn!("Container creation failed, rolling back image '{}'", deployed_image_tag);
            let _ = docker_service::remove_image(&state.docker_client, &deployed_image_tag).await;
            return Err(e);
        }
    };

    let mut tx = state.db_pool.begin().await.map_err(|_| AppError::InternalServerError)?;
    
    let new_project = match project_service::create_project(
        &mut tx,
        &payload.project_name,
        &user_login,
        &container_name,
        source_type,
        &source_url,
        &deployed_image_tag,
        &payload.env_vars,
        &persistent_volume_path,
        &volume_name,
        &state.config.encryption_key,
    ).await
    {
        Ok(project) => project,
        Err(db_error) =>
        {
            warn!("DB persistence failed, rolling back container and image...");
            if let Err(e) = tx.rollback().await
            {
                error!("Failed to rollback transaction. Trying to remove container and image anyway: {}", e);
            }
            let docker = state.docker_client.clone();
            let container_name_clone = container_name.clone();
            let deployed_image_tag_clone = deployed_image_tag.clone();
            tokio::spawn(async move
            {
                // We already log errors inside the functions.
                let _ = docker_service::remove_container(&docker, &container_name_clone).await;
                let _ = docker_service::remove_image(&docker, &deployed_image_tag_clone).await;
            });
            return Err(db_error);
        }
    };

    if payload.create_database.unwrap_or(false)
    {
        if let Err(db_error) = database_service::provision_and_link_database_tx(
            &mut tx,
            &state.mariadb_pool,
            &user_login,
            new_project.id,
            &state.config.encryption_key,
        ).await
        {
            warn!("Database provisioning failed during project creation, rolling back transaction...");
            if let Err(e) = tx.rollback().await
            {
                error!("Failed to rollback transaction. Trying to remove container and image anyway: {}", e);
            }
            let docker = state.docker_client.clone();
            let container_name_clone = container_name.clone();
            let deployed_image_tag_clone = deployed_image_tag.clone();
            tokio::spawn(async move
            {
                // We already log errors inside the functions.
                let _ = docker_service::remove_container(&docker, &container_name_clone).await;
                let _ = docker_service::remove_image(&docker, &deployed_image_tag_clone).await;
            });
            return Err(db_error);
        }
    }

    if let Err(e) = project_service::add_project_participants(&mut tx, new_project.id, &final_participants).await
    {
        warn!("Failed to add participants, rolling back transaction...");
        tx.rollback().await.map_err(|_| AppError::InternalServerError)?;
        return Err(e);
    }

    tx.commit().await.map_err(|_| AppError::InternalServerError)?;

    info!("Project '{}' by user '{}' created successfully.", payload.project_name, user_login);

    let mut project_json = serde_json::to_value(new_project).unwrap_or(json!({}));
    if let Some(obj) = project_json.as_object_mut()
    {
        obj.insert("participants".to_string(), json!(final_participants));
    }

    let response_body = json!({ "project": project_json });
    Ok((StatusCode::CREATED, Json(response_body)))
}

async fn prepare_direct_source(state: &AppState, image_url: &str) -> Result<String, AppError>
{
    info!("Preparing 'direct' source from image '{}'", image_url);
    validation_service::validate_image_url(image_url)?;
    
    let pull_result = docker_service::pull_image(&state.docker_client, image_url, None).await;

    if let Err(e) = pull_result
    {
        if image_url.starts_with("ghcr.io/")
        {
            if let bollard::errors::Error::DockerResponseServerError { status_code, .. } = &e
            {
                if *status_code == 401 || *status_code == 403
                {
                    warn!("Failed to pull private image from ghcr.io: {}", image_url);
                    return Err(ProjectErrorCode::GithubPackageNotPublic.into());
                }
            }
        }
        
        error!("Failed to pull image '{}': {}", image_url, e);
        return Err(ProjectErrorCode::ImagePullFailed.into());
    }
    info!("Successfully pulled public image '{}'", image_url);

    if let Err(scan_error) = docker_service::scan_image_with_grype(image_url, &state.config).await
    {
        warn!("Image scan failed, rolling back by removing pulled image '{}'", image_url);
        let _ = docker_service::remove_image(&state.docker_client, image_url).await;
        return Err(scan_error);
    }

    Ok(image_url.to_string())
}

async fn prepare_github_source(
    state: &AppState,
    project_name: &str,
    repo_url: &str
) -> Result<String, AppError>
{
    info!("Preparing 'github' source for project '{}' from repo '{}'", project_name, repo_url);

    let temp_dir = TempBuilder::new()
        .prefix("hangar-build-")
        .tempdir()
        .map_err(|_| AppError::InternalServerError)?;
    
    match github_service::clone_repo(repo_url, temp_dir.path(), None).await
    {
        Ok(_) =>
        {
            info!("Successfully cloned public repository '{}'", repo_url);
        },
        Err(AppError::ProjectError(ProjectErrorCode::GithubAccountNotLinked)) | Err(AppError::BadRequest(_)) =>
        {
            warn!("Public clone failed for '{}'. Assuming it's a private repo and attempting authenticated clone.", repo_url);

            let (github_owner, repo_name) = github_service::extract_repo_owner_and_name(repo_url).await?;
            let installation_id = github_service::get_installation_id_by_user(&state.http_client, &state.config, &github_owner).await?;
            let token = github_service::get_installation_token(installation_id, &state.http_client, &state.config).await?;
            github_service::check_repo_accessibility(&state.http_client, &token, &github_owner, &repo_name).await?;
            github_service::clone_repo(repo_url, temp_dir.path(), Some(&token)).await?;
            info!("Successfully cloned private repository '{}' using GitHub App token", repo_url);
        },
        Err(e) =>
        {
            return Err(e);
        }
    }

    let dockerfile_content = format!(
        "FROM {}\nCOPY --chown=appuser:appgroup . /var/www/html/",
        &state.config.build_base_image
    );
    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content)
        .map_err(|_| AppError::InternalServerError)?;

    let tarball = docker_service::create_tarball(temp_dir.path())?;
    let image_tag = format!("hangar-local/{}:latest", project_name);
    docker_service::build_image_from_tar(&state.docker_client, tarball, &image_tag).await?;

    if let Err(scan_error) = docker_service::scan_image_with_grype(&image_tag, &state.config).await
    {
        warn!("Image scan failed, rolling back by removing built image '{}'", image_tag);
        let _ = docker_service::remove_image(&state.docker_client, &image_tag).await;
        return Err(scan_error);
    }

    Ok(image_tag)
}

pub async fn purge_project_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError>
{
    let user_login = claims.sub;
    info!("User '{}' initiated purge for project ID: {}", user_login, project_id);

    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, &user_login, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project with ID {} not found or you are not the owner.", project_id)))?;

    info!("Ownership confirmed. Proceeding with purge for project '{}' (ID: {})", project.name, project.id);

    if let Some(db) = database_service::get_database_by_project_id(&state.db_pool, project_id).await?
    {
        info!("Project has a linked database (ID: {}). Deprovisioning it.", db.id);
        database_service::deprovision_database(
            &state.db_pool, 
            &state.mariadb_pool, 
            db.id, 
            &user_login
        ).await?;
        info!("Linked database deprovisioned successfully.");
    }

    docker_service::remove_container(&state.docker_client, &project.container_name).await?;

    if project.persistent_volume_path.is_some()
    {
        let volume_name = match &project.volume_name
        {
            Some(name) => name,
            None => 
            {
                error!("Project '{}' has a persistent volume path but no volume name recorded", project.name);
                return Err(AppError::InternalServerError);
            }
        };
        docker_service::remove_volume_by_name(&state.docker_client, volume_name).await?;
    }

    docker_service::remove_image(&state.docker_client, &project.deployed_image_tag).await?;
    project_service::delete_project_by_id(&state.db_pool, project.id).await?;

    info!("Successfully purged project '{}' for user '{}'.", project.name, user_login);

    Ok((StatusCode::OK, Json(json!({ "status": "success", "message": "Project purged successfully." }))))
}

pub async fn list_owned_projects_handler(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError>
{
    let user_login = claims.sub;
    info!("Fetching owned projects for user '{}'", user_login);
    let projects = project_service::get_projects_by_owner(&state.db_pool, &user_login).await?;
    Ok((StatusCode::OK, Json(json!({ "projects": projects }))))
}

pub async fn list_participating_projects_handler(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError>
{
    let user_login = claims.sub;
    info!("Fetching projects where user '{}' is a participant", user_login);
    let projects = project_service::get_participating_projects(&state.db_pool, &user_login).await?;
    Ok((StatusCode::OK, Json(json!({ "projects": projects }))))
}

pub async fn get_project_details_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<impl IntoResponse, AppError>
{
    let user_login = claims.sub;
    debug!("User '{}' fetching details for project ID: {}", user_login, project_id);

    match project_service::get_project_by_id_for_user(&state.db_pool, project_id, &user_login, claims.is_admin).await?
    {
        Some(mut project) =>
        {
            if let Some(env_vars_value) = &project.env_vars
            {
                let encrypted_vars: HashMap<String, String> = serde_json::from_value(env_vars_value.clone()).unwrap_or_default();
                let decrypted_vars = decrypt_env_vars(&encrypted_vars, &state.config.encryption_key)?;
                project.env_vars = Some(serde_json::to_value(decrypted_vars).unwrap());
            }

            let participants = project_service::get_project_participants(&state.db_pool, project.id).await?;
            let response = ProjectDetailsResponse { project, participants };
            Ok((StatusCode::OK, Json(json!({ "project": response }))))
        }
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
    let project = project_service::get_project_by_id_for_user(&state.db_pool, project_id, &claims.sub, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or access denied.".to_string()))?;
    let status = docker_service::get_container_status(&state.docker_client, &project.container_name).await?;
    Ok(Json(json!({ "status": status.and_then(|s| s.status) })))
}

async fn project_control_handler(
    state: AppState,
    claims: Claims,
    project_id: i32,
    action: ProjectAction,
) -> Result<impl IntoResponse, AppError>
{
    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, &claims.sub, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;
    let status = docker_service::get_container_status(&state.docker_client, &project.container_name).await?;
    if status.is_none() && matches!(action, ProjectAction::Start | ProjectAction::Restart)
    {
        warn!("Container '{}' not found for project ID {}. It might be lost.", project.container_name, project.id);
        return Err(AppError::NotFound(format!("Container for project '{}' seems to be lost. Please contact support or try to redeploy.", project.name)));
    }
    action.execute(state.docker_client.clone(), project.container_name).await?;
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
    let project = project_service::get_project_by_id_for_user(&state.db_pool, project_id, &claims.sub, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or access denied.".to_string()))?;
    let logs = docker_service::get_container_logs(&state.docker_client, &project.container_name, "200").await?;
    Ok(Json(json!({ "logs": logs })))
}

pub async fn get_project_metrics_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
) -> Result<Json<ProjectMetrics>, AppError>
{
    let project = project_service::get_project_by_id_for_user(&state.db_pool, project_id, &claims.sub, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or access denied.".to_string()))?;
    debug!("Fetching metrics for container '{}' (Project ID: {})", project.container_name, project.id);
    let metrics = docker_service::get_container_metrics(&state.docker_client, &project.container_name).await?;
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

    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    if !matches!(project.source, ProjectSourceType::Direct)
    {
        return Err(AppError::BadRequest("Image update is only supported for 'direct' source projects.".to_string()));
    }
    
    let new_image_tag = &payload.new_image_url;
    prepare_direct_source(&state, new_image_tag).await?;
    docker_service::remove_container(&state.docker_client, &project.container_name).await?;

    let decrypted_env_vars = if let Some(env_vars_value) = &project.env_vars
    {
        let encrypted_vars: HashMap<String, String> = serde_json::from_value(env_vars_value.clone()).unwrap_or_default();
        Some(decrypt_env_vars(&encrypted_vars, &state.config.encryption_key)?)
    }
    else
    {
        None
    };

    if let Err(creation_error) = docker_service::create_project_container(
        &state.docker_client,
        &project.name,
        new_image_tag,
        &state.config,
        &decrypted_env_vars,
        &project.persistent_volume_path,
    ).await
    {
        error!("Failed to create new container for project '{}' during update. The service is now down.", project.name);
        let _ = docker_service::remove_image(&state.docker_client, new_image_tag).await;
        return Err(creation_error);
    }

    project_service::update_project_image(&state.db_pool, project.id, new_image_tag).await?;

    let docker_client = state.docker_client.clone();
    let old_image_tag = project.deployed_image_tag;
    
    tokio::spawn(async move
    {
        info!("Attempting to remove old image in background: {}", old_image_tag);
        if let Err(e) = docker_service::remove_image(&docker_client, &old_image_tag).await
        {
            warn!("Could not remove old image '{}' in background: {}", old_image_tag, e);
        }
    });

    info!("Project '{}' image updated successfully by user '{}'.", project.name, user_login);
    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Project image updated successfully."}))))
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

    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login, claims.is_admin)
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

    project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    project_service::remove_participant_from_project(&state.db_pool, project_id, &participant_id).await?;

    info!("Participant '{}' removed successfully from project {}", participant_id, project_id);
    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Participant removed."}))))
}

pub async fn update_env_vars_handler(
    State(state): State<AppState>,
    claims: Claims,
    Path(project_id): Path<i32>,
    Json(payload): Json<UpdateEnvPayload>,
) -> Result<impl IntoResponse, AppError>
{
    let user_login = &claims.sub;
    info!("User '{}' updating environment variables for project ID: {}", user_login, project_id);

    validation_service::validate_env_vars(&payload.env_vars)?;

    let project = project_service::get_project_by_id_and_owner(&state.db_pool, project_id, user_login, claims.is_admin)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found or you are not the owner.".to_string()))?;

    docker_service::remove_container(&state.docker_client, &project.container_name).await?;

    let new_env_vars = Some(payload.env_vars.clone());
    if let Err(creation_error) = docker_service::create_project_container(
        &state.docker_client,
        &project.name,
        &project.deployed_image_tag,
        &state.config,
        &new_env_vars,
        &project.persistent_volume_path,
    ).await
    {
        error!("Failed to recreate container for project '{}' during env update. The service is down.", project.name);
        return Err(creation_error);
    }

    project_service::update_project_env_vars(
        &state.db_pool,
        project.id,
        &payload.env_vars,
        &state.config.encryption_key,
    ).await?;

    info!("Project '{}' environment variables updated and container recreated.", project.name);

    Ok((StatusCode::OK, Json(json!({"status": "success", "message": "Environment variables updated successfully. The project has been restarted."}))))
}

fn decrypt_env_vars(
    encrypted_vars: &HashMap<String, String>,
    key: &[u8],
) -> Result<HashMap<String, String>, AppError>
{
    encrypted_vars.iter()
        .map(|(k, v_b64)|
        {
            let encrypted_val = BASE64_STANDARD.decode(v_b64)
                .map_err(|_| AppError::InternalServerError)?;
            let decrypted_val = crypto_service::decrypt(&encrypted_val, key)?;
            Ok((k.clone(), decrypted_val))
        })
        .collect()
}