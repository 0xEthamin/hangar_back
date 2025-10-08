use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use tracing::{error, trace};

#[derive(Debug, Error)]
pub enum AppError
{
    #[error("Internal Server Error")]
    InternalServerError,

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Error occurred while calling external service")]
    ExternalServiceError(#[from] reqwest::Error),

    #[error("Error parsing response")]
    ParsingError(#[from] quick_xml::DeError),

    #[error("Bad Request: {0}")]
    BadRequest(String),

    #[error("Project operation failed: {0}")]
    ProjectError(#[from] ProjectErrorCode),
}

#[derive(Debug, Error)]
pub enum ConfigError
{
    #[error("Missing environment variable: {0}")]
    Missing(String),

    #[error("Invalid environment variable: {0} (value: '{1}')")]
    Invalid(String, String),
}

#[derive(Debug, Error, Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectErrorCode
{
    #[error("This project name is already taken.")]
    ProjectNameTaken,
    #[error("You already own a project. Only one is allowed per user.")]
    OwnerAlreadyExists,
    #[error("The project owner cannot be added as a participant.")]
    OwnerCannotBeParticipant,
    #[error("The project name is invalid. It must be 1-63 characters, contain only a-z, 0-9, or '-', and not start/end with a hyphen.")]
    InvalidProjectName,
    #[error("The provided Docker image URL is invalid or contains forbidden characters.")]
    InvalidImageUrl,
    #[error("Failed to pull the Docker image. Please check the URL and registry access.")]
    ImagePullFailed,
    #[error("Security scan failed: vulnerabilities were found in the image.")]
    ImageScanFailed(String),
    #[error("Failed to create the project container.")]
    ContainerCreationFailed,
    #[error("Failed to delete the project.")]
    DeleteFailed,
}


impl ProjectErrorCode 
{
    fn as_str(&self) -> &'static str 
    {
        match self 
        {
            ProjectErrorCode::ProjectNameTaken => "PROJECT_NAME_TAKEN",
            ProjectErrorCode::OwnerAlreadyExists => "OWNER_ALREADY_EXISTS",
            ProjectErrorCode::OwnerCannotBeParticipant => "OWNER_CANNOT_BE_PARTICIPANT",
            ProjectErrorCode::InvalidProjectName => "INVALID_PROJECT_NAME",
            ProjectErrorCode::InvalidImageUrl => "INVALID_IMAGE_URL",
            ProjectErrorCode::ImagePullFailed => "IMAGE_PULL_FAILED",
            ProjectErrorCode::ImageScanFailed(_) => "IMAGE_SCAN_FAILED",
            ProjectErrorCode::ContainerCreationFailed => "CONTAINER_CREATION_FAILED",
            ProjectErrorCode::DeleteFailed => "DELETE_FAILED",
        }
    }
}

impl IntoResponse for AppError
{
    fn into_response(self) -> Response
    {
        let (status, body) = match self
        {
            AppError::InternalServerError
            | AppError::ExternalServiceError(_)
            | AppError::ParsingError(_) =>
            {
                error!("--> SERVER ERROR (500): {:?}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error_code": "INTERNAL_SERVER_ERROR", "message": "An internal error has occurred" })),
                )
            }

            AppError::Unauthorized(message) =>
            {
                trace!("--> NOT AUTHORIZED (401): {}", message);
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error_code": "UNAUTHORIZED", "message": message })),
                )
            }

            AppError::NotFound(ressource) =>
            {
                trace!("--> RESOURCE NOT FOUND (404): {}", ressource);
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error_code": "NOT_FOUND", "message": ressource })),
                )
            }

            AppError::BadRequest(message) =>
            {
                trace!("--> BAD REQUEST (400): {}", message);
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error_code": "BAD_REQUEST", "message": message })),
                )
            }
            
            AppError::ProjectError(code) =>
            {
                trace!("--> PROJECT ERROR (400): {}", code);
                let status = match code 
                {
                    ProjectErrorCode::ImagePullFailed | ProjectErrorCode::ContainerCreationFailed => StatusCode::INTERNAL_SERVER_ERROR,
                    _ => StatusCode::BAD_REQUEST
                };

                let mut error_json = json!(
                {
                    "error_code": code.as_str(),
                    "message": code.to_string()
                });

                if let ProjectErrorCode::ImageScanFailed(details) = code 
                {
                    if let Some(obj) = error_json.as_object_mut() 
                    {
                        obj.insert("details".to_string(), json!(details));
                    }
                }

                (
                    status,
                    Json(error_json),
                )
            }
        };

        (status, body).into_response()
    }
}