use std::collections::HashMap;

use pueue_lib::Task;

use crate::{client::helper::*, internal_prelude::*};

/// Set an environment variable and make sure it's there afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_environment() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a stashed task so we can edit it.
    run_client_command(shared, &["add", "--stashed", "echo $TEST_VARIABLE"])?;

    // Set the environment variable
    run_client_command(shared, &["env", "set", "0", "TEST_VARIABLE", "thisisatest"])?;

    // Now start the command and wait for it to finish
    run_client_command(shared, &["enqueue", "0"])?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let state = get_state(shared).await?;
    println!("{:#?}", state.tasks[&0].envs);

    // Make sure the environment variable has been set.
    let output = run_client_command(shared, &["follow", "0"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!("thisisatest", stdout.trim());

    Ok(())
}

/// Task environments inherit only explicitly selected non-default variables by default.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selective_environment_inheritance() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    let envs = HashMap::from([
        ("PUEUE_AGENT_SECRET", "do-not-copy"),
        ("PUEUE_AGENT_SELECTED", "selected"),
    ]);

    run_client_command_with_env(
        shared,
        &[
            "add",
            "--stashed",
            "--env",
            "PUEUE_AGENT_SELECTED",
            "echo test",
        ],
        envs.clone(),
    )?
    .success()?;
    run_client_command_with_env(
        shared,
        &["add", "--stashed", "--inherit-env", "echo test"],
        envs.clone(),
    )?
    .success()?;
    run_client_command_with_env(
        shared,
        &[
            "add",
            "--stashed",
            "--env",
            "PUEUE_AGENT_SET=value",
            "echo test",
        ],
        envs.clone(),
    )?
    .success()?;

    let state = get_state(shared).await?;
    assert_eq!(state.tasks[&0].envs["PUEUE_AGENT_SELECTED"], "selected");
    assert!(!state.tasks[&0].envs.contains_key("PUEUE_AGENT_SECRET"));
    assert_eq!(state.tasks[&1].envs["PUEUE_AGENT_SECRET"], "do-not-copy");
    assert_eq!(state.tasks[&2].envs["PUEUE_AGENT_SET"], "value");
    assert!(!state.tasks[&2].envs.contains_key("PUEUE_AGENT_SECRET"));

    let output = run_client_command(shared, &["add", "--env", "PUEUE_AGENT_MISSING", "echo"])?;
    assert!(!output.status.success());
    assert_eq!(get_state(shared).await?.tasks.len(), 3);

    let output = run_client_command_with_env(
        shared,
        &[
            "-vvvv",
            "add",
            "--stashed",
            "--env",
            "TRACE_SECRET=trace-secret",
            "echo test",
        ],
        envs,
    )?
    .success()?;
    assert!(!String::from_utf8_lossy(&output.stderr).contains("trace-secret"));

    Ok(())
}

/// Set an environment variable, immediately unset it and make sure it's not there afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unset_environment() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a stashed task so we can edit it.
    run_client_command(shared, &["add", "--stashed", "echo $TEST_VARIABLE"])?;

    // Set the environment variable
    run_client_command(shared, &["env", "set", "0", "TEST_VARIABLE", "thisisatest"])?;

    // Unset the environment variable again.
    run_client_command(shared, &["env", "unset", "0", "TEST_VARIABLE"])?;

    // Now start the command and wait for it to finish
    run_client_command(shared, &["enqueue", "0"])?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let state = get_state(shared).await?;
    println!("{:#?}", state.tasks[&0].envs);

    // Make sure the environment variable has been set.
    let output = run_client_command(shared, &["follow", "0"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!("", stdout.trim());

    Ok(())
}
