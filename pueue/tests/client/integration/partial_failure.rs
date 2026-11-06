use crate::{client::helper::*, internal_prelude::*};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn valid_ids_are_processed_but_the_command_fails() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    run_client_command(shared, &["add", "--stashed", "echo test"])?;

    let output = run_client_command(shared, &["remove", "0", "999"])?;
    assert!(!output.status.success());
    assert!(get_state(shared).await?.tasks.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("999"));

    Ok(())
}
