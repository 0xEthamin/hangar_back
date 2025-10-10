use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type)]
#[sqlx(type_name = "project_source_type", rename_all = "lowercase")]
pub enum ProjectSourceType 
{
    Direct,
    Github,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Project 
{
    pub id: i32,
    pub name: String,
    pub owner: String,

    pub container_name: String,

    #[sqlx(rename = "source_type")]
    pub source: ProjectSourceType,

    pub source_url: String,
    pub deployed_image_tag: String,

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectMetrics 
{
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub memory_limit: f64,
}