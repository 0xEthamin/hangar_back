use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, TokenData};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims 
{
    pub sub: String,
    pub name: String,
    pub email: String,
    pub exp: i64,
    pub is_admin: bool,
}

pub fn generate_jwt(secret: &str, jwt_expiration_seconds : u64, login: &str, name: &str, email: &str, is_admin: bool) -> Result<String, AppError> 
{
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let claims = Claims 
    {
        sub: login.to_string(),
        name: name.to_string(),
        email: email.to_string(),
        exp: (now + jwt_expiration_seconds) as i64,
        is_admin,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).map_err(|_| AppError::InternalServerError)
}

pub fn validate_jwt(token: &str, secret: &str) -> Result<TokenData<Claims>, AppError> 
{
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default())
    .map_err(|_| AppError::Unauthorized("Invalid token".to_string()))
}
