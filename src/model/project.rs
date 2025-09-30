use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)] 
pub struct Project 
{
    pub id: i32,
    pub name: String,
    pub owner: String,
    pub image_url: String,
    pub container_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectDetailsResponse 
{
    #[serde(flatten)]
    pub project: Project,
    pub participants: Vec<String>,
}
