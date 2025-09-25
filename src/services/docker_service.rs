use bollard::secret::ResourcesUlimits;
use bollard::Docker;
use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::
{
    CreateContainerOptionsBuilder, CreateImageOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions, RemoveImageOptions
};
use futures::stream::StreamExt;
use tokio::process::Command;
use std::collections::HashMap;
use std::process::Stdio;
use tracing::{error, info, warn};

use crate::error::AppError;

pub async fn pull_image(docker: &Docker, image_url: &str) -> Result<(), AppError> 
{
    let options = Some(CreateImageOptions 
    {
        from_image: Some(image_url.to_string()),
        ..Default::default()
    });

    let mut stream = docker.create_image(options, None, None);

    while let Some(result) = stream.next().await 
    {
        match result 
        {
            Ok(info) => 
            {
                if let Some(status) = info.status 
                {
                    tracing::debug!("Pulling image {}: {}", image_url, status);
                }
            }
            Err(e) => 
            {
                error!("Failed to pull image '{}': {}", image_url, e);
                return Err(AppError::BadRequest(format!("Failed to pull image: {}", e)));
            }
        }
    }
    info!("Image '{}' pulled successfully.", image_url);
    Ok(())
}

pub async fn scan_image_with_grype(image_url: &str, config: &crate::config::Config) -> Result<(), AppError> 
{
    info!("Scanning image '{}' with Grype...", image_url);

    let mut command = Command::new("grype");
    command
        .arg(image_url)
        .arg("--only-fixed")
        .arg("--fail-on")
        .arg(&config.grype_fail_on_severity)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command.output().await.map_err(|e| 
    {
        error!("Failed to execute grype command: {}", e);
        AppError::InternalServerError
    })?;

    if !output.status.success() 
    {
        warn!("Grype found vulnerabilities in image '{}'", image_url);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let error_message = format!(
            "Image scan failed. Grype found high severity vulnerabilities.\n--- STDOUT ---\n{}\n--- STDERR ---\n{}",
            stdout, stderr
        );
        return Err(AppError::BadRequest(error_message));
    }

    info!("Grype scan passed for image '{}'.", image_url);
    Ok(())
}

pub async fn create_project_container(docker: &Docker, project_name: &str, image_url: &str, config: &crate::config::Config) -> Result<String, AppError> 
{
    let container_name = format!("{}-{}", &config.app_prefix, project_name);
    let hostname = format!("{}.{}", project_name, &config.app_domain_suffix);

    let host_config = HostConfig 
    {
        memory: Some(config.container_memory_mb * 1024 * 1024),
        cpu_quota: Some(config.container_cpu_quota),
        //network_mode: Some(config.docker_network.clone()),
        security_opt: Some(vec![
            "no-new-privileges:true".to_string(),
            "apparmor:docker-default".to_string()
        ]),
        readonly_rootfs: Some(false),
        privileged: Some(false),
        pids_limit: Some(256),
        ulimits: Some(vec![
            ResourcesUlimits { name: Some("nofile".to_string()), soft: Some(1024), hard: Some(2048) },
            ResourcesUlimits { name: Some("nproc".to_string()), soft: Some(64), hard: Some(128) }
        ]),
        
        // Montages sécurisés
        tmpfs: Some(HashMap::from([
            ("/tmp".to_string(), "rw,noexec,nosuid,size=100m".to_string())
        ])),
        oom_kill_disable: Some(false),
        memory_swappiness: Some(0),
        ..Default::default()
    };

    let mut labels = HashMap::new();
    labels.insert("app".to_string(), config.app_prefix.clone());
    labels.insert("traefik.enable".to_string(), "true".to_string());
    labels.insert(format!("traefik.http.routers.{}.rule", project_name), format!("Host(`{}`)", hostname));
    labels.insert(format!("traefik.http.routers.{}.entrypoints", project_name), config.traefik_entrypoint.clone());
    labels.insert(format!("traefik.http.routers.{}.tls.certresolver", project_name), config.traefik_cert_resolver.clone());
    labels.insert(format!("traefik.http.services.{}.loadbalancer.server.port", project_name), "80".to_string());

    let config = ContainerCreateBody 
    {
        image: Some(image_url.to_string()),
        host_config: Some(host_config),
        labels: Some(labels),
        ..Default::default()
    };

    let options = Some(CreateContainerOptionsBuilder::new().name(&container_name).build());

    let response = docker.create_container(options, config).await.map_err(|e| 
    {
        error!("Failed to create container '{}': {}", container_name, e);
        AppError::InternalServerError
    })?;

    docker.start_container(&container_name, None::<StartContainerOptions>).await.map_err(|e| 
    {
        error!("Failed to start container '{}': {}", container_name, e);
        
        let docker_clone = docker.clone();
        let container_name_clone = container_name.clone();
        
        tokio::spawn(async move 
        {
            warn!("Attempting rollback for failed container start: {}", container_name_clone);
            if let Err(remove_err) = docker_clone.remove_container(&container_name_clone, None::<RemoveContainerOptions>).await 
            {
                error!("ROLLBACK FAILED: Could not remove container '{}' after start failure: {}", container_name_clone, remove_err);
            } 
            else 
            {
                info!("Rollback successful for container '{}'", container_name_clone);
            }
        });
        
        AppError::InternalServerError
    })?;

    info!("Container '{}' created and started with ID: {}", container_name, response.id);
    Ok(response.id)
}

pub async fn remove_container(docker: &Docker, container_name: &str) -> Result<(), AppError> 
{
    info!("Attempting to stop and remove container: {}", container_name);

    match docker.stop_container(container_name, None::<StopContainerOptions>).await 
    {
        Ok(_) => (),
        Err(bollard::errors::Error::DockerResponseServerError { status_code, .. }) if status_code == 404 || status_code == 304 =>
        {
            warn!("Container {} not found or already stopped. No action taken.", container_name);
        },
        Err(e) => 
        {
            error!("Error stopping container {}: {}", container_name, e);
        }
    }

    match docker.remove_container(container_name, None::<RemoveContainerOptions>).await 
    {
        Ok(_) => (),
        Err(bollard::errors::Error::DockerResponseServerError { status_code, .. }) if status_code == 404 => 
        {
            warn!("Container {} not found during removal. It might have been deleted already.", container_name);
        },
        Err(e) =>
        {
            error!("Error removing container {}: {}", container_name, e);
            return Err(AppError::InternalServerError);
        }
    }

    info!("Container {} has been successfully removed.", container_name);
    Ok(())
}

pub async fn remove_image(docker: &Docker, image_url: &str) -> Result<(), AppError>
{
    info!("Attempting to remove image: {}", image_url);

    let options = Some(RemoveImageOptions 
    {
        force: true,
        ..Default::default()
    });
    if let Err(e) = docker.remove_image(image_url, options, None).await 
    {
        error!("Could not remove image '{}': {}", image_url, e);
        Err(AppError::InternalServerError)
    } 
    else 
    {
        info!("Image {} successfully removed", image_url);
        Ok(())
    }
}