use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError 
{
    #[error("Internal Server Error")]
    InternalServerError,
    
    #[error("Ressource non trouvée: {0}")]
    NotFound(String),
}

impl IntoResponse for AppError 
{
    fn into_response(self) -> Response 
    {
        let (status, error_message) = match self 
        {
            AppError::InternalServerError => 
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Une erreur interne est survenue".to_string(),
            ),

            AppError::NotFound(ressource) =>
            (
                StatusCode::NOT_FOUND,
                format!("Ressource non trouvée: {}", ressource),
            ),
        };

        let body = Json(json!(
        {
            "error": error_message,
        }));

        (status, body).into_response()
    }
}