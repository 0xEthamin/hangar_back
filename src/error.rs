use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;
use tracing::{error, warn, info};

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
                error!("--> ERREUR SERVEUR (5xx): {:?}", self); 
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Une erreur interne est survenue".to_string(),
                )
            }

            AppError::Unauthorized(message) => 
            {
                warn!("--> REQUÊTE NON AUTORISÉE (401): {}", message);
                (
                    StatusCode::UNAUTHORIZED,
                    format!("Non autorisé: {}", message),
                )
            }

            AppError::NotFound(ressource) =>
            {
                info!("--> RESSOURCE NON TROUVÉE (404): {}", ressource);
                (
                    StatusCode::NOT_FOUND,
                    format!("Ressource non trouvée: {}", ressource),
                )
            }
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}