use sqlx::{PgPool, Postgres, Transaction};
use tracing::error;
use crate::{error::AppError, model::project::Project};

pub async fn check_project_name_exists(pool: &PgPool, name: &str) -> Result<bool, AppError> 
{
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE name = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::InternalServerError)?;
    Ok(count.0 > 0)
}

pub async fn check_owner_exists(pool: &PgPool, owner: &str) -> Result<bool, AppError> 
{
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE owner = $1")
        .bind(owner)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::InternalServerError)?;
    Ok(count.0 > 0)
}

pub async fn create_project<'a>(
    tx: &mut Transaction<'a, Postgres>,
    name: &str,
    owner: &str,
    image_url: &str,
    container_id: &str,
) -> Result<Project, AppError> 
{
    let project = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (name, owner, image_url, container_id) VALUES ($1, $2, $3, $4) RETURNING *"
    )
        .bind(name)
        .bind(owner)
        .bind(image_url)
        .bind(container_id)
        .fetch_one(&mut **tx) 
        .await
        .map_err(|e: sqlx::Error| 
        {
            error!("Failed to create project in DB: {}", e);
            if let Some(db_err) = e.as_database_error() 
            {
                if db_err.is_unique_violation() 
                {
                    return AppError::BadRequest("Project name or owner already exists.".to_string());
                }
            }
            AppError::InternalServerError
        })?;

    Ok(project)
}

pub async fn delete_project_by_id(pool: &PgPool, project_id: i32) -> Result<(), AppError> 
{
    let result = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(project_id)
        .execute(pool)
        .await
        .map_err(|e| 
        {
            error!("Failed to delete project with id '{}': {}", project_id, e);
            AppError::InternalServerError
        })?;

    if result.rows_affected() == 0 
    {
        return Err(AppError::NotFound(format!("Project with id {} not found for deletion.", project_id)));
    }

    Ok(())
}

// Note : On retourne un Vec<Project> pour être prêt pour le futur,
// même si la logique actuelle ne permet qu'un projet par owner.
pub async fn get_projects_by_owner(pool: &PgPool, owner: &str) -> Result<Vec<Project>, AppError> 
{
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE owner = $1 ORDER BY created_at DESC")
        .bind(owner)
        .fetch_all(pool)
        .await
        .map_err(|e| 
        {
            error!("Failed to fetch projects for owner '{}': {}", owner, e);
            AppError::InternalServerError
        })
}

pub async fn get_project_by_id_and_owner(
    pool: &PgPool,
    project_id: i32,
    owner: &str,
) -> Result<Option<Project>, AppError> 
{
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1 AND owner = $2")
        .bind(project_id)
        .bind(owner)
        .fetch_optional(pool)
        .await
        .map_err(|e| 
        {
            error!("Failed to fetch project by id {} and owner '{}': {}", project_id, owner, e);
            AppError::InternalServerError
        })
}

pub async fn get_participating_projects(pool: &PgPool, participant_id: &str) -> Result<Vec<Project>, AppError> 
{
    sqlx::query_as::<_, Project>(
        "SELECT p.* FROM projects p
         JOIN project_participants pp ON p.id = pp.project_id
         WHERE pp.participant_id = $1
         ORDER BY p.created_at DESC"
    )
        .bind(participant_id)
        .fetch_all(pool)
        .await
        .map_err(|e| 
        {
            error!("Failed to fetch participating projects for user '{}': {}", participant_id, e);
            AppError::InternalServerError
        })
}