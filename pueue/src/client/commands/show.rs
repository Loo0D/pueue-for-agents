use std::collections::BTreeMap;

use pueue_lib::{
    Client, Task,
    message::{Response, TasksRequest},
};
use serde::Serialize;

use crate::internal_prelude::*;

pub async fn show(client: &mut Client, task_ids: Vec<usize>, include_env: bool) -> Result<()> {
    client
        .send_request(TasksRequest {
            task_ids,
            include_env,
        })
        .await?;
    let tasks = match client.receive_response().await? {
        Response::Tasks(tasks) => tasks,
        Response::Failure(message) => bail!(message),
        response => bail!("Unexpected task response: {response:?}"),
    };

    let output = ShowOutput {
        schema_version: 1,
        tasks,
    };
    println!("{}", serde_json::to_string(&output).unwrap());
    Ok(())
}

#[derive(Serialize)]
struct ShowOutput {
    schema_version: u8,
    tasks: BTreeMap<usize, Task>,
}
