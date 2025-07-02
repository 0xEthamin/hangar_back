use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config 
{
    pub host: String,
    pub port: u16,
}

impl Config 
{
    pub fn from_env() -> Result<Self, std::env::VarError> 
    {
        let host = std::env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("APP_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse::<u16>()
            .expect("APP_PORT must be a valid u16");

        Ok(Config { host, port })
    }
}