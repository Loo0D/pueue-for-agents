use pueue_lib::Task;

use crate::{client::helper::*, internal_prelude::*};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closed_confirmation_input_aborts_without_panicking() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    add_and_start_task(shared, "sleep 60").await?;
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    let output = run_client_command(shared, &["reset"])?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Confirmation input was closed"));
    assert!(!stderr.contains("panicked"));
    assert_eq!(get_state(shared).await?.tasks.len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_waits_for_completion_by_default() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    add_and_start_task(shared, "sleep 60").await?;
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    run_client_command(shared, &["reset", "--force"])?.success()?;
    assert!(get_state(shared).await?.tasks.is_empty());

    Ok(())
}
