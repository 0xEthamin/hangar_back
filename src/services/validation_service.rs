use crate::error::{AppError, ProjectErrorCode};
use std::collections::HashSet;

pub fn validate_project_name(name: &str) -> Result<(), AppError>
{
    if name.is_empty() 
    {
        return Err(ProjectErrorCode::InvalidProjectName.into());
    }
    if name.len() > 63 
    {
        return Err(ProjectErrorCode::InvalidProjectName.into());
    }

    let is_valid_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !is_valid_chars 
    {
        return Err(ProjectErrorCode::InvalidProjectName.into());
    }

    if name.starts_with('-') || name.ends_with('-') 
    {
        return Err(ProjectErrorCode::InvalidProjectName.into());
    }

    Ok(())
}

pub fn validate_image_url(url: &str) -> Result<(), AppError> 
{
    if url.is_empty() 
    {
        return Err(ProjectErrorCode::InvalidImageUrl.into());
    }

    let forbidden_chars: HashSet<char> = " $`'\"\\".chars().collect();
    if url.chars().any(|c| forbidden_chars.contains(&c)) 
    {
        return Err(ProjectErrorCode::InvalidImageUrl.into());
    }
    Ok(())
}