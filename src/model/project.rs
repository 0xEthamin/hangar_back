use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)] 
pub struct Project 
{
    #[serde(skip)]
    #[serde(rename = "id")]
    pub _id: i32,
    pub name: String,
    pub owner: String,
    pub participants: Vec<String>,
    pub image_url: String,
    pub container_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}