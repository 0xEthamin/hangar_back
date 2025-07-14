mod config;
mod error;
mod handlers;
mod router;
mod state;
mod services;
mod model;
mod middleware;

use crate::config::Config;
use crate::state::InnerState;
use std::net::{SocketAddr, Ipv4Addr};
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() 
{
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let config = match Config::from_env() 
    {
        Ok(config) => config,
        Err(e) => 
        {
            tracing::error!("❌ Configuration error: {}", e);
            std::process::exit(1); // On quitte proprement
        }
    };



    let app_state = InnerState::new(config.clone());
    let app = router::create_router(app_state);

    let addr = SocketAddr::from((config.host.parse::<Ipv4Addr>().unwrap(), config.port));
    info!("🚀 Server listening on http://{}", addr);

    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("🔗 Listening on: {}", addr);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}