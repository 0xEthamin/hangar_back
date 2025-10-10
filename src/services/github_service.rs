use crate::{config::Config, error::{AppError, ProjectErrorCode}};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::process::Command;
use tracing::{debug, error, info};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};


#[derive(Debug, Deserialize)]
struct Installation
{
    id: u64,
    account: Account,
}


#[derive(Debug, Deserialize)]
struct Account
{
    login: String,
}

#[derive(Debug, Serialize)]
struct AppJwtClaims
{
    iat: u64, // Issued at
    exp: u64, // Expiration time
    iss: String, // Issuer (App ID)
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse
{
    token: String,
}
pub async fn extract_github_username(repo_url: &str) -> Result<String, AppError>
{
    let url = repo_url.trim();
    
    if !url.contains("github.com") 
    {
        return Err(AppError::BadRequest(
            "Only GitHub repositories are supported. URL must contain 'github.com'.".to_string()
        ));
    }
    
    let url_without_protocol = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    
    let parts: Vec<&str> = url_without_protocol
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split('/')
        .collect();

    if parts.len() < 3 
    {
        return Err(AppError::BadRequest(
            "Invalid GitHub repository URL format. Expected: https://github.com/username/repository".to_string()
        ));
    }
    
    if !parts[0].eq_ignore_ascii_case("github.com") 
    {
        return Err(AppError::BadRequest(
            "Invalid GitHub URL. Must start with 'github.com'.".to_string()
        ));
    }

    let username = parts[1];
    
    if username.is_empty() 
    {
        return Err(AppError::BadRequest(
            "GitHub username cannot be empty in the repository URL.".to_string()
        ));
    }
    
    let reserved_keywords = ["orgs", "organizations", "settings", "marketplace", "explore"];
    if reserved_keywords.contains(&username) 
    {
        return Err(AppError::BadRequest(
            format!("'{}' is not a valid GitHub username.", username)
        ));
    }
    
    info!("Extracted GitHub username '{}' from URL '{}'", username, repo_url);
    Ok(username.to_string())
}
async fn generate_app_jwt(config: &Config) -> Result<String, AppError>
{
    let now = OffsetDateTime::now_utc().unix_timestamp() as u64;
    let claims = AppJwtClaims 
    {
        iat: now - 60,        // 60 secondes dans le passé pour éviter les problèmes de synchronisation d'horloge
        exp: now + (10 * 60), // Le token est valide pour 10 minutes maximum
        iss: config.github_app_id.clone(),
    };
    let header = Header::new(Algorithm::RS256);
    let key = EncodingKey::from_rsa_pem(&config.github_private_key).map_err(|e| 
    {
        error!("Failed to create encoding key from RSA PEM: {}", e);
        AppError::InternalServerError
    })?;

    encode(&header, &claims, &key).map_err(|e| 
    {
        error!("Failed to encode GitHub App JWT: {}", e);
        AppError::InternalServerError
    })
}


pub async fn get_installation_id_by_user(http_client: &reqwest::Client, config: &Config, github_username: &str) -> Result<u64, AppError>
{
    let app_jwt = generate_app_jwt(config).await?;

    let response = http_client
        .get("https://api.github.com/app/installations")
        .header("Authorization", format!("Bearer {}", app_jwt))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Hangar App")
        .send()
        .await?;

    if !response.status().is_success()
    {
        error!("Failed to fetch installations from GitHub.");
        return Err(AppError::InternalServerError);
    }

    let installations_response: Vec<Installation> = response.json().await?;

    for inst in installations_response
    {
        if inst.account.login.eq_ignore_ascii_case(github_username)
        {
            debug!("Found matching GitHub App installation with ID: {} for user {}", inst.id, github_username);
            return Ok(inst.id);
        }
    }

    Err(ProjectErrorCode::GithubAccountNotLinked.into())
}

pub async fn get_installation_token(installation_id: u64, http_client: &reqwest::Client, config: &Config) -> Result<String, AppError>
{
    let app_jwt = generate_app_jwt(config).await?;
    let url = format!("https://api.github.com/app/installations/{}/access_tokens", installation_id);

    let response = http_client
        .post(&url)
        .header("Authorization", format!("Bearer {}", app_jwt))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Hangar App")
        .send()
        .await?;
    
    if !response.status().is_success()
    {
        let error_body = response.text().await.unwrap_or_default();
        error!("GitHub installation token request failed: {}", error_body);
        return Err(AppError::InternalServerError);
    }

    let token_response: InstallationTokenResponse = response.json().await?;
    Ok(token_response.token)
}

pub async fn clone_repo(repo_url: &str, target_dir: &std::path::Path, token: &str) -> Result<(), AppError>
{

    let authenticated_url = repo_url.replace("https://", &format!("https://x-access-token:{}@", token));

    let output = Command::new("git")
        .arg("clone")
        .arg("--depth=1")
        .arg(authenticated_url)
        .arg(target_dir)
        .output()
        .await
        .map_err(|e| 
        {
            error!("Failed to execute git clone command: {}", e);
            AppError::InternalServerError
        })?;

    if !output.status.success()
    {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Failed to clone repository '{}': {}", repo_url, stderr);
        return Err(AppError::BadRequest("Failed to clone repository. Check if the URL is correct and if the App has access.".to_string()));
    }

    info!("Repository {} cloned successfully.", repo_url);
    Ok(())
}

