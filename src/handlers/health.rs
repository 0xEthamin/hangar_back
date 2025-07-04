use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::error::AppError;

pub async fn health_check_handler() -> Result<impl IntoResponse, AppError>
{
    Ok((StatusCode::OK, "OK"))
}

pub async fn error_check_handler() -> Result<impl IntoResponse, AppError>
{
    Err::<(), AppError>(AppError::InternalServerError)
}

pub async fn not_found_handler() ->  Result<impl IntoResponse, AppError> 
{
    Err::<(), AppError>(AppError::NotFound("Test 404".to_string()))
}