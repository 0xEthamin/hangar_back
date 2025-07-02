use crate::{handlers, state::AppState};
use axum::{routing::get, Router, error_handling::HandleErrorLayer, BoxError, http::StatusCode};
use tower::{timeout::TimeoutLayer, ServiceBuilder};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use std::time::Duration;

pub fn create_router(state: AppState) -> Router 
{
    

    Router::new()
        .route("/api/health", get(handlers::health::health_check_handler))
        .route("/api/error", get(handlers::health::error_check_handler))
        .route("/api/not_found", get(handlers::health::not_found_handler))
        .with_state(state)
        .layer
        (
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
                .layer(CompressionLayer::new())
                .layer(HandleErrorLayer::new(|_: BoxError| async 
                {
                    StatusCode::REQUEST_TIMEOUT
                }))
                .layer(TimeoutLayer::new(Duration::from_secs(10)))
        )
}

