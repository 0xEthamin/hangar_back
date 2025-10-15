use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Database
{
    pub id: i32,
    pub owner_login: String,
    pub database_name: String,
    pub username: String,
    pub encrypted_password: String,
    pub project_id: Option<i32>,

    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct DatabaseDetailsResponse
{
    pub id: i32,
    pub owner_login: String,
    pub project_id: Option<i32>,
    pub database_name: String,
    pub username: String,
    pub password: String, // Mot de passe en clair
    pub host: String,
    pub port: u16,
    
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}