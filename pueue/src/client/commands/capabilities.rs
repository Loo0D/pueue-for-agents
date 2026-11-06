use pueue_lib::{Client, Request, Response};
use serde::Serialize;

use crate::internal_prelude::*;

#[derive(Serialize)]
struct Capabilities {
    json_schema_version: u8,
    client_version: &'static str,
    daemon_version: String,
    protocol_version: String,
    shell: ShellCapabilities,
    features: Vec<&'static str>,
}

#[derive(Serialize)]
struct ShellCapabilities {
    source: String,
    command: Vec<String>,
}

pub async fn capabilities(client: &mut Client) -> Result<()> {
    client.send_request(Request::Capabilities).await?;
    let server = match client.receive_response().await? {
        Response::Capabilities(response) => response,
        Response::Failure(message) => bail!(message),
        response => bail!("Unexpected capabilities response: {response:?}"),
    };

    let output = Capabilities {
        json_schema_version: 1,
        client_version: env!("CARGO_PKG_VERSION"),
        daemon_version: server.daemon_version,
        protocol_version: server.protocol_version,
        shell: ShellCapabilities {
            source: server.shell_source,
            command: server.shell_command,
        },
        features: vec![
            "add_batch",
            "byte_bounded_logs",
            "head_tail_logs",
            "dependency_preserving_restart",
            "dependent_restart",
            "json_wait",
            "reset_wait",
            "selective_environment_capture",
            "targeted_task_show",
        ],
    };
    println!("{}", serde_json::to_string(&output).unwrap());
    Ok(())
}
