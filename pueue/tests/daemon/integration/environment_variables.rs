use pueue_lib::task::Task;

use crate::{helper::*, internal_prelude::*};

/// Make sure that the daemon's environment variables don't bleed into the spawned subprocesses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_isolated_task_environment() -> Result<()> {
    let (settings, _tempdir) = daemon_base_setup()?;
    let mut child = standalone_daemon(&settings.shared).await?;

    let shared = &settings.shared;

    // Spawn a task which prints a special environment variable.
    // This environment variable is injected into the daemon's environment.
    // It shouldn't show up in the task's environment, as the task should be isolated!
    assert_success(add_and_start_task(shared, "echo $PUEUED_TEST_ENV_VARIABLE").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let log = get_task_log(shared, 0, None).await?;

    // The log output should be empty
    assert_eq!(log, "\n");

    child.kill()?;
    Ok(())
}

/// Without a daemon override, a task uses the shell captured from its client environment.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_task_uses_captured_shell() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let (mut settings, tempdir) = daemon_base_setup()?;
    settings.daemon.shell_command = None;
    let custom_shell = tempdir.path().join("custom-shell");
    std::fs::write(
        &custom_shell,
        "#!/bin/sh\nprintf 'custom-shell\\n'\nexec /bin/sh -c \"$2\"\n",
    )?;
    std::fs::set_permissions(&custom_shell, std::fs::Permissions::from_mode(0o700))?;
    settings.save(&Some(tempdir.path().join("pueue.yml")))?;
    let daemon = daemon_with_settings(settings, tempdir).await?;
    let shared = &daemon.settings.shared;

    let mut message = create_add_message(shared, "echo task-output");
    message
        .envs
        .insert("SHELL".into(), custom_shell.to_string_lossy().into_owned());
    assert_success(send_request(shared, message).await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let log = get_task_log(shared, 0, None).await?;
    assert_eq!(log, "custom-shell\ntask-output\n");

    Ok(())
}
