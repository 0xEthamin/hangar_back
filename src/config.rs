use crate::error::ConfigError;
use serde::Deserialize;
use base64::prelude::*;

#[derive(Deserialize, Clone)]
pub struct Config
{
    pub host: String,
    pub port: u16,
    pub db_url: String,
    pub public_address: String,
    pub jwt_secret: String,
    pub jwt_expiration_seconds: u64,
    pub cas_validation_url: String,
    pub app_prefix: String,
    pub app_domain_suffix: String,
    pub build_base_image: String,
    pub github_app_id: String,
    pub github_private_key: Vec<u8>,
    pub docker_network: String,
    pub traefik_entrypoint: String,
    pub traefik_cert_resolver: String,
    pub container_memory_mb: i64,
    pub container_cpu_quota: i64,
    pub grype_fail_on_severity: String,
    pub db_max_connections: u32,
    pub timeout_normal: u64,
    pub timeout_long: u64,
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

        let public_address = std::env::var("APP_PUBLIC_ADDRESS")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        let db_url = std::env::var("DATABASE_URL")
            .map_err(|_| ConfigError::Missing("DATABASE_URL".to_string()))?;

        let jwt_secret = std::env::var("APP_JWT_SECRET")
            .map_err(|_| ConfigError::Missing("APP_JWT_SECRET".to_string()))?;

        let jwt_expiration_seconds = std::env::var("JWT_EXPIRATION_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3600);

        let cas_validation_url = std::env::var("CAS_VALIDATION_URL")
            .map_err(|_| ConfigError::Missing("CAS_VALIDATION_URL".to_string()))?;

        let app_prefix = std::env::var("APP_PREFIX").unwrap_or_else(|_| "hangar".to_string());
        let app_domain_suffix =
            std::env::var("APP_DOMAIN_SUFFIX").unwrap_or_else(|_| "localhost".to_string());

        let build_base_image = std::env::var("BUILD_BASE_IMAGE")
            .map_err(|_| ConfigError::Missing("BUILD_BASE_IMAGE".to_string()))?;

        let github_app_id = std::env::var("GITHUB_APP_ID")
            .map_err(|_| ConfigError::Missing("GITHUB_APP_ID".to_string()))?;

        let private_key_b64 = std::env::var("GITHUB_PRIVATE_KEY_B64")
            .map_err(|_| ConfigError::Missing("GITHUB_PRIVATE_KEY_B64".to_string()))?;

        let github_private_key = BASE64_STANDARD.decode(private_key_b64)
            .map_err(|_| ConfigError::Invalid("GITHUB_PRIVATE_KEY_B64".to_string(), "Invalid Base64".to_string()))?;

        let docker_network =
            std::env::var("DOCKER_NETWORK").unwrap_or_else(|_| "traefik-net".to_string());
        let traefik_entrypoint = std::env::var("DOCKER_TRAEFIK_ENTRYPOINT")
            .unwrap_or_else(|_| "websecure".to_string());
        let traefik_cert_resolver = std::env::var("DOCKER_TRAEFIK_CERTRESOLVER")
            .unwrap_or_else(|_| "myresolver".to_string());
        let grype_fail_on_severity = std::env::var("GRYPE_FAIL_ON_SEVERITY")
            .unwrap_or_else(|_| "high".to_string());

        let container_memory_mb = std::env::var("DOCKER_CONTAINER_MEMORY_MB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(512);

        let container_cpu_quota = std::env::var("DOCKER_CONTAINER_CPU_QUOTA")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50000);

        let db_max_connections = std::env::var("DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let timeout_normal = std::env::var("TIMEOUT_SECONDS_NORMAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let timeout_long = std::env::var("TIMEOUT_SECONDS_LONG")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);

        Ok(Config 
        {
            host,
            port,
            db_url,
            public_address,
            jwt_secret,
            jwt_expiration_seconds,
            cas_validation_url,
            app_prefix,
            app_domain_suffix,
            build_base_image,
            github_app_id,
            github_private_key,
            docker_network,
            traefik_entrypoint,
            traefik_cert_resolver,
            container_memory_mb,
            container_cpu_quota,
            grype_fail_on_severity,
            db_max_connections,
            timeout_normal,
            timeout_long,
        })
    }
}