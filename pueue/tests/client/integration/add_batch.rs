use pueue_lib::{Task, TaskResult, TaskStatus};

use crate::{client::helper::*, internal_prelude::*};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adds_a_named_dependency_graph() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    let batch_path = shared.pueue_directory().join("batch.json");
    std::fs::write(
        &batch_path,
        r#"{
          "tasks": [
            {"name":"merge", "dependencies":["parse"], "command":"cat scan.txt"},
            {"name":"parse", "dependencies":["scan"], "command":"sed s/scan/parsed/ scan.txt > parsed.txt"},
            {"name":"scan", "command":"echo scan > scan.txt", "label":"producer"}
          ]
        }"#,
    )?;

    let output = run_client_command(
        shared,
        &["add-batch", batch_path.to_string_lossy().as_ref()],
    )?
    .success()?;
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["task_ids"]["scan"], 0);
    assert_eq!(result["task_ids"]["parse"], 1);
    assert_eq!(result["task_ids"]["merge"], 2);

    let task = wait_for_task_condition(shared, 2, Task::is_done).await?;
    assert!(matches!(
        task.status,
        TaskStatus::Done {
            result: TaskResult::Success,
            ..
        }
    ));
    assert_eq!(
        std::fs::read_to_string(shared.pueue_directory().join("parsed.txt"))?,
        "parsed\n"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_graph_is_not_added() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    let batch_path = shared.pueue_directory().join("cycle.json");
    std::fs::write(
        &batch_path,
        r#"{
          "tasks": [
            {"name":"a", "dependencies":["b"], "command":"echo a"},
            {"name":"b", "dependencies":["a"], "command":"echo b"}
          ]
        }"#,
    )?;

    let output = run_client_command(
        shared,
        &["add-batch", batch_path.to_string_lossy().as_ref()],
    )?;
    assert!(!output.status.success());
    assert!(get_state(shared).await?.tasks.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn save_failure_rolls_back_the_batch() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    let batch_path = shared.pueue_directory().join("valid.json");
    std::fs::write(
        &batch_path,
        r#"{"tasks":[{"name":"task", "stashed":true, "command":"echo test"}]}"#,
    )?;

    // A directory at the temporary state-file path forces the daemon's save to fail.
    std::fs::create_dir(shared.pueue_directory().join("state.json.partial"))?;
    let output = run_client_command(
        shared,
        &["add-batch", batch_path.to_string_lossy().as_ref()],
    )?;
    assert!(!output.status.success());
    assert!(get_state(shared).await?.tasks.is_empty());

    Ok(())
}
