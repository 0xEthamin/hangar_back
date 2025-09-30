use crate::{handlers, state::AppState, middleware};
use axum::{error_handling::HandleErrorLayer, http::StatusCode, middleware as axum_middleware, routing::{delete, get, post}, BoxError, Router};
use tower::{timeout::TimeoutLayer, ServiceBuilder};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use std::time::Duration;

pub fn create_router(state: AppState) -> Router 
{
    let common_layer = ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
                .layer(CompressionLayer::new())
                .layer(HandleErrorLayer::new(|_: BoxError| async {StatusCode::REQUEST_TIMEOUT}))
                .layer(TimeoutLayer::new(Duration::from_secs(state.config.timeout_normal)));

    let long_running_layer = ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
                .layer(CompressionLayer::new())
                .layer(HandleErrorLayer::new(|_: BoxError| async {StatusCode::REQUEST_TIMEOUT}))
                .layer(TimeoutLayer::new(Duration::from_secs(state.config.timeout_long)));

    let public_routes = Router::new()
        .route("/api/health", get(handlers::health::health_check_handler))
        .route("/api/error", get(handlers::health::error_check_handler))
        .route("/api/not-found", get(handlers::health::not_found_handler))
        .route("/api/auth/callback", get(handlers::auth_handler::auth_callback_handler))
        .route_layer(common_layer.clone());

    let protected_routes = Router::new()
        .route("/api/auth/me", get(handlers::auth_handler::get_current_user_handler))
        .route("/api/auth/logout", get(handlers::auth_handler::logout_handler))
        .route("/api/projects/owned", get(handlers::project_handler::list_owned_projects_handler))
        .route("/api/projects/participations", get(handlers::project_handler::list_participating_projects_handler))
        .route("/api/projects/{project_id}", get(handlers::project_handler::get_project_details_handler))
        .route("/api/projects/{project_id}/status", get(handlers::project_handler::get_project_status_handler))
        .route("/api/projects/{project_id}/start", post(handlers::project_handler::start_project_handler))
        .route("/api/projects/{project_id}/stop", post(handlers::project_handler::stop_project_handler))
        .route("/api/projects/{project_id}/restart", post(handlers::project_handler::restart_project_handler))
        .route_layer(axum_middleware::from_fn_with_state(state.clone(), middleware::auth))
        .route_layer(common_layer.clone());

    let long_running_protected_routes = Router::new()
        .route("/api/projects/deploy", post(handlers::project_handler::deploy_project_handler))
        .route("/api/projects/{project_id}", delete(handlers::project_handler::purge_project_handler))
        .route_layer(axum_middleware::from_fn_with_state(state.clone(), middleware::auth))
        .route_layer(long_running_layer);

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(long_running_protected_routes)
        .with_state(state)
}

