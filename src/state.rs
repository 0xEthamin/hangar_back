use std::sync::Arc;

pub type AppState = Arc<InnerState>;

#[derive(Clone)]
pub struct InnerState 
{
    // Ici, vous ajouteriez plus tard :
    // pub db_pool: sqlx::PgPool,
    // pub http_client: reqwest::Client,
}

impl InnerState 
{
    pub fn new() -> AppState 
    {
        Arc::new(Self {})
    }
}