use std::{
    io::{self, Read},
    path::{Path, PathBuf},
};

use pueue_lib::{
    Client,
    message::{AddBatchRequest, AddBatchTask, Response},
};
use serde::{Deserialize, Serialize};

use crate::internal_prelude::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchDocument {
    tasks: Vec<AddBatchTask>,
}

pub async fn add_batch(
    client: &mut Client,
    file: Option<PathBuf>,
    environment: Vec<String>,
    inherit_env: bool,
) -> Result<()> {
    let json = match file {
        Some(path) if path != Path::new("-") => std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read batch file {}", path.display()))?,
        _ => {
            let mut json = String::new();
            io::stdin().lock().read_to_string(&mut json)?;
            json
        }
    };
    let mut document: BatchDocument = serde_json::from_str(&json)?;

    let current_directory = std::env::current_dir()?;
    for task in &mut document.tasks {
        task.path = if task.path.as_os_str().is_empty() {
            current_directory.clone()
        } else {
            task.path
                .canonicalize()
                .with_context(|| format!("Invalid path for batch task {:?}", task.name))?
        };
    }

    client
        .send_request(AddBatchRequest {
            tasks: document.tasks,
            envs: super::add::task_environment(environment, inherit_env)?,
        })
        .await?;

    match client.receive_response().await? {
        Response::AddedBatch(response) => {
            let output = BatchOutput {
                schema_version: 1,
                task_ids: response.task_ids,
            };
            println!("{}", serde_json::to_string(&output).unwrap());
            Ok(())
        }
        Response::Failure(message) => bail!(message),
        response => bail!("Unexpected response while adding a batch: {response:?}"),
    }
}

#[derive(Serialize)]
struct BatchOutput {
    schema_version: u8,
    task_ids: std::collections::BTreeMap<String, usize>,
}
