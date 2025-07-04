use axum::
{
    extract::{Request, State, FromRequestParts},
    http::request::Parts,
    middleware::Next,
    response::Response,
};
use axum_extra::extract::CookieJar;

use crate::
{
    error::AppError,
    services::jwt::{self, Claims},
    state::AppState,
};

pub async fn auth(State(state): State<AppState>,jar: CookieJar, mut req: Request, next: Next) -> Result<Response, AppError> 
{
   
    let token = jar.get("auth_token").map(|cookie| cookie.value())
        .ok_or_else(|| AppError::Unauthorized("Token d'authentification manquant.".to_string()))?;

    let token_data = jwt::validate_jwt(token, &state.config.jwt_secret)?;

    req.extensions_mut().insert(token_data.claims);

    Ok(next.run(req).await)
}

impl<S> FromRequestParts<S> for Claims where S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> 
    {
        parts.extensions.get::<Claims>().cloned().ok_or_else(|| 
        {
            tracing::error!("L'extracteur de Claims a été utilisé sur une route non protégée par le middleware d'authentification.");
            AppError::InternalServerError
        })
    }
}