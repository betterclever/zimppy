use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Response, Server, StatusCode};
use zimppy_core::ReservedPorts;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalAppConfig {
    project_name: String,
    ports: LocalPorts,
    storage: StorageConfig,
    services: LocalServices,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalPorts {
    api: u16,
    backend: u16,
    test_helper: u16,
    integration_harness: u16,
    lightwalletd_tunnel: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageConfig {
    state_directory: String,
    sqlite_file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalServices {
    api_base_url: String,
    backend_base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RemoteChainServiceConfig {
    network: String,
    lightwalletd: LightwalletdConfig,
    upstream: UpstreamConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LightwalletdConfig {
    access: String,
    host: String,
    port: u16,
    endpoint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamConfig {
    host_alias: String,
    remote_port: u16,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    local_app: LocalAppConfig,
    remote_chain_service: RemoteChainServiceConfig,
    backend_port: u16,
    state_directory: PathBuf,
}

impl RuntimeConfig {
    fn load() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let local_app: LocalAppConfig = read_json(repo_root.join("config/local-app.json"))?;
        let remote_chain_service: RemoteChainServiceConfig =
            read_json(repo_root.join("config/remote-chain-service.json"))?;
        let backend_port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(local_app.ports.backend);
        let state_directory = repo_root.join(&local_app.storage.state_directory);

        fs::create_dir_all(&state_directory)?;

        Ok(Self {
            local_app,
            remote_chain_service,
            backend_port,
            state_directory,
        })
    }

    fn health_body(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&serde_json::json!({
            "service": "zimppy-backend",
            "status": "ok",
            "project": self.local_app.project_name,
            "ports": {
                "api": self.local_app.ports.api,
                "backend": self.backend_port,
                "testHelper": self.local_app.ports.test_helper,
                "integrationHarness": self.local_app.ports.integration_harness,
                "lightwalletdTunnel": self.local_app.ports.lightwalletd_tunnel,
            },
            "remoteChainService": {
                "network": self.remote_chain_service.network,
                "endpoint": self.remote_chain_service.lightwalletd.endpoint,
            },
            "storage": {
                "stateDirectory": self.state_directory.display().to_string(),
                "sqliteFile": self.local_app.storage.sqlite_file,
            }
        }))
    }
}

fn read_json<T>(path: impl AsRef<Path>) -> Result<T, Box<dyn Error + Send + Sync>>
where
    T: for<'de> Deserialize<'de>,
{
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn json_header() -> Result<Header, Box<dyn Error + Send + Sync>> {
    Header::from_bytes(b"Content-Type", b"application/json").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "failed to build static JSON content-type header",
        )
        .into()
    })
}

fn handle_request(
    request: tiny_http::Request,
    config: &RuntimeConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let response = if request.method() == &Method::Get && request.url() == "/health" {
        Response::from_string(config.health_body()?)
            .with_status_code(StatusCode(200))
            .with_header(json_header()?)
    } else {
        Response::from_string(serde_json::to_string(&serde_json::json!({
            "type": "about:blank",
            "title": "Not Found",
            "status": 404,
            "detail": format!("No route for {} {}", request.method(), request.url()),
        }))?)
        .with_status_code(StatusCode(404))
        .with_header(json_header()?)
    };

    request.respond(response)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = RuntimeConfig::load()?;
    let reserved_ports = ReservedPorts::new();
    let bind_address = format!("127.0.0.1:{}", config.backend_port);
    let server = Server::http(&bind_address)?;

    println!(
        "zimppy-backend listening on http://{bind_address} (api {} / tunnel {})",
        reserved_ports.api, reserved_ports.lightwalletd_tunnel
    );

    for request in server.incoming_requests() {
        if let Err(error) = handle_request(request, &config) {
            eprintln!("backend request handling failed: {error}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RuntimeConfig;

    #[test]
    fn loads_backend_runtime_config_with_reserved_ports() {
        let config = RuntimeConfig::load().unwrap_or_else(|error| {
            panic!("runtime config should load: {error}");
        });

        assert_eq!(config.local_app.project_name, "zimppy");
        assert_eq!(config.local_app.ports.backend, 3181);
        assert_eq!(config.local_app.ports.lightwalletd_tunnel, 3184);
        assert_eq!(config.remote_chain_service.network, "testnet");
        assert!(config.state_directory.ends_with(".local/state/zimppy"));
    }
}
