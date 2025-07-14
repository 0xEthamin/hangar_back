use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;
use tracing::{error, trace};

#[derive(Debug, Error)]
pub enum AppError 
{
    #[error("Internal Server Error")]
    InternalServerError,

    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Error occurred while calling external service")]
    ExternalServiceError(#[from] reqwest::Error),

    #[error("Error parsing response")]
    ParsingError(#[from] quick_xml::DeError),
}

#[derive(Debug, Error)]
pub enum ConfigError 
{
    #[error("Missing environment variable: {0}")]
    Missing(String),
    
    #[error("Invalid environment variable: {0} (value: '{1}')")]
    Invalid(String, String),
}

impl IntoResponse for AppError 
{
    fn into_response(self) -> Response 
    {
        let (status, error_message) = match &self 
        {
            
            AppError::InternalServerError | AppError::ExternalServiceError(_) | AppError::ParsingError(_) => 
            {
                error!("--> SERVER ERROR (5xx): {:?}", self); 
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal error has occurred".to_string(),
                )
            }

            AppError::Unauthorized(message) => 
            {
                trace!("--> NOT AUTHORIZED (401): {}", message);
                (
                    StatusCode::UNAUTHORIZED,
                    format!("not authorized: {}", message),
                )
            }

            AppError::NotFound(ressource) =>
            {
                trace!("--> RESOURCE NOT FOUND (404): {}", ressource);
                (
                    StatusCode::NOT_FOUND,
                    format!("Resource not found: {}", ressource),
                )
            }
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}