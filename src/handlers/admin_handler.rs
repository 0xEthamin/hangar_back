use axum::{extract::State, response::Json, response::IntoResponse};
use serde_json::json;
use crate::{error::AppError, services::{docker_service, project_service}, state::AppState};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use crate::model::project::DownProjectInfo;

pub async fn list_all_projects_handler(
    State(state): State<AppState>
) -> Result<impl IntoResponse, AppError> 
{
    let projects = project_service::get_all_projects(&state.db_pool).await?;
    Ok(Json(json!({ "projects": projects })))
}

pub async fn get_global_metrics_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> 
{

    let mut metrics = docker_service::get_global_container_stats(
        &state.docker_client,
        &state.config.app_prefix,
    ).await?;
    
    let projects = project_service::get_all_projects(&state.db_pool).await?;
    metrics.total_projects = projects.len() as i64;

    Ok(Json(metrics))
}

pub async fn get_down_projects_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> 
{
    let all_projects = project_service::get_all_projects(&state.db_pool).await?;
    let mut down_projects: Vec<DownProjectInfo> = Vec::new();

    let now = OffsetDateTime::now_utc();

    for project in all_projects 
    {
        if let Some(details) = docker_service::inspect_container_details(&state.docker_client, &project.container_name).await?
            && let Some(container_state) = details.state
                && let Some(is_running) = container_state.running
                    && !is_running
                        && let Some(finished_at_str) = container_state.finished_at
                            && let Ok(stopped_at) = OffsetDateTime::parse(&finished_at_str, &Rfc3339)
                            {
                                let downtime_seconds = (now - stopped_at).as_seconds_f64() as i64;
                                down_projects.push(DownProjectInfo 
                                {
                                    project: project.clone(),
                                    stopped_at: finished_at_str,
                                    downtime_seconds,
                                });
                            }
    }

    down_projects.sort_by(|a, b| b.downtime_seconds.cmp(&a.downtime_seconds));

    Ok(Json(json!({ "down_projects": down_projects })))
}