use crate::{client::helper::*, internal_prelude::*};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_tasks_and_redaction() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    run_client_command(
        shared,
        &[
            "add",
            "--stashed",
            "--env",
            "PUEUE_AGENT_SHOW=value",
            "echo test",
        ],
    )?;

    let output = run_client_command(shared, &["show", "0"])?.success()?;
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["tasks"]["0"]["command"], "echo test");
    assert_eq!(result["tasks"]["0"]["envs"], serde_json::json!({}));

    let output = run_client_command(shared, &["show", "--include-env", "0"])?.success()?;
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["tasks"]["0"]["envs"]["PUEUE_AGENT_SHOW"], "value");

    let output = run_client_command(shared, &["show", "0", "999"])?;
    assert!(!output.status.success());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capabilities() -> Result<()> {
    let (mut settings, tempdir) = daemon_base_setup()?;
    settings.daemon.shell_command = None;
    settings.save(&Some(tempdir.path().join("pueue.yml")))?;
    let daemon = daemon_with_settings(settings, tempdir).await?;

    let output = run_client_command(&daemon.settings.shared, &["capabilities"])?.success()?;
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["json_schema_version"], 1);
    assert_eq!(result["client_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(result["daemon_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(result["protocol_version"], pueue_lib::PROTOCOL_VERSION);
    assert_eq!(result["shell"]["source"], "task_shell_environment");
    assert!(
        result["features"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("add_batch"))
    );

    Ok(())
}
