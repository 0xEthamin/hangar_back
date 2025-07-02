mod config;
mod error;
mod handlers;
mod router;
mod state;

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

    let config = Config::from_env().expect("Failed to load configuration");

    let app_state = InnerState::new();
    let app = router::create_router(app_state);

    let addr = SocketAddr::from((config.host.parse::<Ipv4Addr>().unwrap(), config.port));
    info!("🚀 Server listening on http://{}", addr);

    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}