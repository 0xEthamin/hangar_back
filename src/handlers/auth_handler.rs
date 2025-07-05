use axum::
{
    extract::{MatchedPath, Query, State}, 
    response::{IntoResponse, Json}
};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, state::AppState};
use crate::services::jwt::Claims;

#[derive(Debug, Deserialize)]
pub struct AuthCallbackQuery 
{
    ticket: String,
}

pub async fn auth_callback_handler(State(state): State<AppState>, 
                                   Query(query): Query<AuthCallbackQuery>, 
                                   jar: CookieJar,
                                   matched_path: MatchedPath) -> Result<impl IntoResponse, AppError>
{
    let service = format!("{}{}", state.config.public_address, matched_path.as_str());

    let url = format!("{}?service={}&ticket={}", state.config.cas_validation_url, service, &query.ticket);
    let user = crate::services::auth_service::validate_ticket(&url, &state.http_client).await?;

    let token = crate::services::jwt::generate_jwt(
        &state.config.jwt_secret,
        &user.login,
        &user.email,
    )?;

    let cookie = Cookie::build(("auth_token", token.to_string()))
        .path("/") // Le cookie est valide pour tout le site
        .secure(true) // Envoyé seulement sur HTTPS
        .http_only(true) // Inaccessible depuis JavaScript
        .same_site(SameSite::Lax) // Protection CSRF de base
        .build();
    
    Ok((
        jar.add(cookie),
        Json
        (
            json!
            (
                {
                    "message": "Authentication successful",
                    "user": 
                    {
                        "login": user.login,
                        "email": user.email
                    }
                }
            )
        ),
    ))

}

pub async fn get_current_user_handler(claims: Claims) -> impl IntoResponse 
{
    Json
    (
        json!
        (
            {
                "user": 
                {
                    "login": claims.sub,
                    "email": claims.email
                }
            }
        )
    )
}