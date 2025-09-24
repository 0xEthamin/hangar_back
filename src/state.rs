use std::sync::Arc;
use bollard::Docker;
use sqlx::PgPool;
use crate::config::Config;

pub type AppState = Arc<InnerState>;

pub struct InnerState 
{
    pub config : Config,
    pub http_client: reqwest::Client,
    pub docker_client: Docker,
    pub db_pool: PgPool,
}

impl InnerState 
{
    pub fn new(config: Config, docker_client: Docker, db_pool: PgPool) -> AppState 
    {
        Arc::new(Self 
        {
            config,
            http_client: reqwest::Client::new(),
            docker_client,
            db_pool,
        })
    }
}