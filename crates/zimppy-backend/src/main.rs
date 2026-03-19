use tiny_http::{Header, Method, Response, Server, StatusCode};
use zimppy_backend::{DynError, RemoteLightwalletdVerifier, RuntimeConfig};
use zimppy_core::ReservedPorts;

fn json_header() -> Result<Header, DynError> {
    Header::from_bytes(b"Content-Type", b"application/json").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "failed to build static JSON content-type header",
        )
        .into()
    })
}

fn handle_request(request: tiny_http::Request, config: &RuntimeConfig) -> Result<(), DynError> {
    let response = if request.method() == &Method::Get && request.url() == "/health" {
        Response::from_string(config.health_body()?)
            .with_status_code(StatusCode(200))
            .with_header(json_header()?)
    } else if request.method() == &Method::Get && request.url() == "/remote-chain/connectivity" {
        let verifier = RemoteLightwalletdVerifier::new(&config.remote_chain_service);

        match verifier.check_connectivity() {
            Ok(connectivity) => Response::from_string(serde_json::to_string_pretty(&connectivity)?)
                .with_status_code(StatusCode(200))
                .with_header(json_header()?),
            Err(error) => Response::from_string(serde_json::to_string(&serde_json::json!({
                "type": "about:blank",
                "title": "Remote lightwalletd unavailable",
                "status": 503,
                "detail": error.to_string(),
            }))?)
            .with_status_code(StatusCode(503))
            .with_header(json_header()?),
        }
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

fn run_connectivity_check() -> Result<(), DynError> {
    let config = RuntimeConfig::load()?;
    let verifier = RemoteLightwalletdVerifier::new(&config.remote_chain_service);
    let connectivity = verifier.check_connectivity()?;
    println!("{}", serde_json::to_string_pretty(&connectivity)?);
    Ok(())
}

fn main() -> Result<(), DynError> {
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("check-remote-lightwalletd")
    ) {
        return run_connectivity_check();
    }

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
