use serde::Deserialize;
use tracing::error;
use crate::error::AppError;
use crate::model::user::User;

#[derive(Debug, Deserialize)]
struct ServiceResponse {
    #[serde(rename = "authenticationSuccess", alias = "cas:authenticationSuccess")]
    authentication_success: Option<AuthenticationSuccess>,
}

#[derive(Debug, Deserialize)]
struct AuthenticationSuccess 
{
    #[serde(rename = "attributes", alias = "cas:attributes")]
    attributes: Option<CasAttributes>,
}

#[derive(Debug, Deserialize)]
struct CasAttributes 
{
    #[serde(rename = "mail", alias = "cas:mail")]
    mail: Option<String>,

    #[serde(rename = "prenom", alias = "cas:prenom")]
    prenom: Option<String>,

    #[serde(rename = "login", alias = "cas:login")]
    login: Option<String>,
}


pub async fn validate_ticket(url: &str, client: &reqwest::Client)  -> Result<User, AppError>
{

    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        error!("The CAS service responded with an error status: {}", response.status());
        return Err(AppError::Unauthorized("The authentication service refused validation.".to_string()));
    }

    let xml_body = response.text().await?;

    tracing::debug!("CAS response body: {}", xml_body);

    let service_response: ServiceResponse = quick_xml::de::from_str(&xml_body)?;

    let auth = service_response.authentication_success
        .ok_or_else(|| { AppError::Unauthorized("Invalid ticket".to_string()) })?;

    let attributes = auth.attributes
        .ok_or_else(|| { AppError::Unauthorized("Missing attributes".to_string()) })?;

    let email = attributes.mail
        .ok_or_else(|| { error!("Missing mail in CAS"); AppError::Unauthorized("Missing mail".to_string()) })?;

    let login = attributes.login
        .ok_or_else(|| { error!("Missing login in CAS"); AppError::Unauthorized("Missing login".to_string()) })?;

    let prenom = attributes.prenom
        .ok_or_else(|| { error!("Missing prenom in CAS"); AppError::Unauthorized("Missing prenom".to_string()) })?;

    Ok(User { email, name : prenom, login })
}