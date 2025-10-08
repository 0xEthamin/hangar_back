use bollard::secret::{ContainerState, ContainerStatsResponse, ResourcesUlimits, RestartPolicy};
use bollard::Docker;
use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::
{
    CreateContainerOptionsBuilder, CreateImageOptions, InspectContainerOptions, LogsOptions, RemoveContainerOptions, RemoveImageOptions, RestartContainerOptions, StartContainerOptions, StatsOptions, StopContainerOptions
};
use futures::stream::StreamExt;
use tokio::process::Command;
use std::collections::HashMap;
use std::process::Stdio;
use tracing::{debug, error, info, warn};

use crate::error::{AppError, ProjectErrorCode};
use crate::model::project::ProjectMetrics;

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
                return Err(ProjectErrorCode::ImagePullFailed.into());
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
        let report = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(ProjectErrorCode::ImageScanFailed(report).into());
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
        restart_policy: Some(RestartPolicy 
        {
            name: Some(bollard::secret::RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),

        memory: Some(config.container_memory_mb * 1024 * 1024),
        cpu_quota: Some(config.container_cpu_quota),
        network_mode: Some(config.docker_network.clone()),
        security_opt: Some(vec![
            "no-new-privileges:true".to_string(),
            "apparmor:docker-default".to_string()
        ]),
        readonly_rootfs: Some(false),
        privileged: Some(false),
        pids_limit: Some(1024),
        ulimits: Some(vec![
            ResourcesUlimits { name: Some("nofile".to_string()), soft: Some(1024), hard: Some(2048) },
            ResourcesUlimits { name: Some("nproc".to_string()), soft: Some(512), hard: Some(1024) }
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
    //labels.insert(format!("traefik.http.routers.{}.tls.certresolver", project_name), config.traefik_cert_resolver.clone());
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
        ProjectErrorCode::ContainerCreationFailed
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
        
        ProjectErrorCode::ContainerCreationFailed
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

pub async fn get_container_status(docker: &Docker, container_name: &str) -> Result<Option<ContainerState>, AppError> 
{
    match docker.inspect_container(container_name, None::<InspectContainerOptions>).await 
    {
        Ok(details) => Ok(details.state),
        Err(bollard::errors::Error::DockerResponseServerError { status_code, .. }) if status_code == 404 => 
        {
            Ok(None)
        },
        Err(e) => 
        {
            error!("Failed to inspect container '{}': {}", container_name, e);
            Err(AppError::InternalServerError)
        }
    }
}

pub async fn start_container_by_name(docker: &Docker, container_name: &str) -> Result<(), AppError> 
{
    docker.start_container(container_name, None::<StartContainerOptions>).await.map_err(|e| 
    {
        error!("Failed to start container '{}': {}", container_name, e);
        AppError::InternalServerError
    })
}

pub async fn stop_container_by_name(docker: &Docker, container_name: &str) -> Result<(), AppError> 
{
    docker.stop_container(container_name, None::<StopContainerOptions>).await.map_err(|e| 
    {
        error!("Failed to stop container '{}': {}", container_name, e);
        AppError::InternalServerError
    })
}

pub async fn restart_container_by_name(docker: &Docker, container_name: &str) -> Result<(), AppError>
{
    docker.restart_container(container_name, None::<RestartContainerOptions>).await.map_err(|e| 
    {
        error!("Failed to restart container '{}': {}", container_name, e);
        AppError::InternalServerError
    })
}

pub async fn get_container_logs(docker: &Docker, container_name: &str, tail: &str) -> Result<String, AppError> 
{
    info!("Fetching logs for container '{}' with tail '{}'", container_name, tail);

    let options = Some(LogsOptions 
    {
        stdout: true,
        stderr: true,
        tail: tail.to_string(),
        timestamps: true,
        ..Default::default()
    });

    let mut stream = docker.logs(container_name, options);

    let mut log_entries = Vec::new();
    while let Some(log_result) = stream.next().await 
    {
        match log_result 
        {
            Ok(log_output) => log_entries.push(log_output.to_string()),
            Err(e) => 
            {
                error!("Error streaming logs for container '{}': {}", container_name, e);
            }
        }
    }

    Ok(log_entries.join(""))
}

pub async fn get_container_metrics(docker: &Docker, container_name: &str) -> Result<ProjectMetrics, AppError> 
{
    let mut stream = docker.stats(container_name, Some(StatsOptions 
    { 
        stream: false, 
        ..Default::default() 
    }));

    if let Some(stats_result) = stream.next().await 
    {
        match stats_result 
        {
            Ok(stats) => 
            {
                debug!("Received stats for container '{}': {:?}", container_name, stats);
                
                let cpu_usage = calculate_cpu_percent(&stats);
                let (memory_usage, memory_limit) = calculate_memory(&stats);

                Ok(ProjectMetrics 
                {
                    cpu_usage,
                    memory_usage: memory_usage as f64,
                    memory_limit: memory_limit as f64,
                })
            }
            Err(e) => 
            {
                error!("Failed to get stats for container '{}': {}", container_name, e);
                Err(AppError::InternalServerError)
            }
        }
    } 
    else 
    {
        Err(AppError::NotFound(format!("No stats received for container {}", container_name)))
    }
}

fn calculate_cpu_percent(stats: &ContainerStatsResponse) -> f64 
{

    let calculation = || -> Option<f64> 
    {
        let cpu_stats = stats.cpu_stats.as_ref()?;
        let precpu_stats = stats.precpu_stats.as_ref()?;

        let cpu_usage = cpu_stats.cpu_usage.as_ref()?;
        let precpu_usage = precpu_stats.cpu_usage.as_ref()?;

        let total_usage = cpu_usage.total_usage?;
        let pre_total_usage = precpu_usage.total_usage?;

        let cpu_delta = total_usage as f64 - pre_total_usage as f64;

        let system_cpu_delta = (cpu_stats.system_cpu_usage? as f64) - (precpu_stats.system_cpu_usage? as f64);

        let number_of_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;

        if system_cpu_delta > 0.0 && cpu_delta > 0.0 
        {
            Some((cpu_delta / system_cpu_delta) * number_of_cpus * 100.0)
        } 
        else 
        {
            Some(0.0)
        }
    }();

    calculation.unwrap_or(0.0)
}

fn calculate_memory(stats: &ContainerStatsResponse) -> (u64, u64) 
{
    if let Some(mem_stats) = stats.memory_stats.as_ref() 
    {
        let usage = mem_stats.usage.unwrap_or(0);
        let limit = mem_stats.limit.unwrap_or(0);

        let cache = mem_stats.stats.as_ref()
            .and_then(|s| s.get("cache"))
            .map_or(0, |v| *v);

        let actual_usage = usage.saturating_sub(cache);
        (actual_usage, limit)
    } 
    else 
    {
        (0, 0)
    }
}