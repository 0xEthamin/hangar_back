use crate::error::AppError;
use std::collections::HashSet;

pub fn validate_project_name(name: &str) -> Result<(), AppError>
{
    if name.is_empty() 
    {
        return Err(AppError::BadRequest("Project name cannot be empty.".to_string()));
    }
    if name.len() > 63 
    {
        return Err(AppError::BadRequest("Project name cannot exceed 63 characters.".to_string()));
    }

    let is_valid_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !is_valid_chars 
    {
        return Err(AppError::BadRequest("Project name can only contain letters, numbers, and hyphens.".to_string()));
    }

    if name.starts_with('-') || name.ends_with('-') 
    {
        return Err(AppError::BadRequest("Project name cannot start or end with a hyphen.".to_string()));
    }

    Ok(())
}

pub fn validate_image_url(url: &str) -> Result<(), AppError> 
{
    if url.is_empty() 
    {
        return Err(AppError::BadRequest("Image URL cannot be empty.".to_string()));
    }

    let forbidden_chars: HashSet<char> = " $`'\"\\".chars().collect();
    if url.chars().any(|c| forbidden_chars.contains(&c)) 
    {
        return Err(AppError::BadRequest("Image URL contains invalid characters.".to_string()));
    }

    if !url.contains(|c: char| c == '/' || c == ':') && !url.contains('@') 
    {
        // Do nothing for now
        // Example below to allow some exceptions
        
        //if url.to_lowercase() == "scratch" { return Ok(()); } 
    }

    Ok(())
}