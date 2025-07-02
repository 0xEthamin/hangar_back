use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::error::AppError;

pub async fn health_check_handler() -> impl IntoResponse 
{
    (StatusCode::OK, "OK")
}

pub async fn error_check_handler() -> impl IntoResponse 
{
    AppError::InternalServerError.into_response()
}

pub async fn not_found_handler() -> impl IntoResponse 
{
    AppError::NotFound("Test 404".to_string()).into_response()
}