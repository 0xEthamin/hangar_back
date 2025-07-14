use std::sync::Arc;

use crate::config::Config;

pub type AppState = Arc<InnerState>;

#[derive(Clone)]
pub struct InnerState 
{
    pub config : Config,
    pub http_client: reqwest::Client,
}

impl InnerState 
{
    pub fn new(config: Config) -> AppState 
    {
        Arc::new(Self 
        { 
            config,
            http_client: reqwest::Client::new()
        })
    }
}