use serde::Deserialize;
use crate::error::ConfigError;

#[derive(Debug, Deserialize, Clone)]
pub struct Config 
{
    pub host: String,
    pub port: u16,
    pub public_address: String,
    pub jwt_secret: String,
    pub jwt_expiration_seconds: u64,
    pub cas_validation_url: String,
}

impl Config 
{
    pub fn from_env() -> Result<Self, ConfigError> 
    {
        let host = std::env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        
        let port_str = std::env::var("APP_PORT").unwrap_or_else(|_| "3000".to_string());
        let port = port_str.parse::<u16>().map_err(|_| 
        {
            ConfigError::Invalid("APP_PORT".to_string(), port_str)
        })?;

        let public_address = std::env::var("APP_PUBLIC_ADDRESS").unwrap_or_else(|_| "http://localhost:8080".to_string());

        let jwt_secret = std::env::var("APP_JWT_SECRET").map_err(|_| 
        {
            ConfigError::Missing("APP_JWT_SECRET".to_string())
        })?;
        
        let jwt_expiration_seconds = std::env::var("JWT_EXPIRATION_SECONDS").ok()
                                                                                    .and_then(|s| s.parse::<u64>().ok())
                                                                                    .unwrap_or(3600);

        let cas_validation_url = std::env::var("CAS_VALIDATION_URL").map_err(|_| 
        {
            ConfigError::Missing("CAS_VALIDATION_URL".to_string())
        })?;

        Ok(Config { host, port, public_address, jwt_secret, jwt_expiration_seconds, cas_validation_url })
    }
}