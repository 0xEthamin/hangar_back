use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, TokenData};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;

const EXPIRATION_SECONDS: u64 = 60 * 60; // 1 heure

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims 
{
    pub sub: String,
    pub email: String,
    pub exp: usize,
}

pub fn generate_jwt(secret: &str, login: &str, email: &str) -> Result<String, AppError> 
{
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let claims = Claims 
    {
        sub: login.to_string(),
        email: email.to_string(),
        exp: (now + EXPIRATION_SECONDS) as usize,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).map_err(|_| AppError::InternalServerError)
}

pub fn validate_jwt(token: &str, secret: &str) -> Result<TokenData<Claims>, AppError> 
{
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default())
    .map_err(|_| AppError::Unauthorized("Invalid token".to_string()))
}
