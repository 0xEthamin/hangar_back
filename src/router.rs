use crate::{handlers, state::AppState, middleware};
use axum::{routing::get, Router, error_handling::HandleErrorLayer, BoxError, http::StatusCode, middleware as axum_middleware};
use tower::{timeout::TimeoutLayer, ServiceBuilder};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use std::time::Duration;

pub fn create_router(state: AppState) -> Router 
{
    let public_routes = Router::new()
        .route("/api/health", get(handlers::health::health_check_handler))
        .route("/api/error", get(handlers::health::error_check_handler))
        .route("/api/not-found", get(handlers::health::not_found_handler))
        .route("/api/auth/callback", get(handlers::auth_handler::auth_callback_handler));

    let protected_routes = Router::new()
        .route("/api/auth/me", get(handlers::auth_handler::get_current_user_handler))
        .route("/api/auth/logout", get(handlers::auth_handler::logout_handler))
        .route_layer(axum_middleware::from_fn_with_state(state.clone(), middleware::auth));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
        .layer
        (
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
                .layer(CompressionLayer::new())
                .layer(HandleErrorLayer::new(|_: BoxError| async {StatusCode::REQUEST_TIMEOUT}))
                .layer(TimeoutLayer::new(Duration::from_secs(10)))
        )
}

