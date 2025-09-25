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
    pub created_at: OffsetDateTime,
}