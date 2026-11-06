use std::collections::{BTreeMap, HashMap};

use pueue_lib::Task;
use rstest::rstest;
use serde::Deserialize;

use crate::{client::helper::*, internal_prelude::*};

/// Test that the `log` command works for both:
/// - The log being streamed by the daemon.
/// - The log being read from the local files.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Force the client to read remote logs via config file.
    daemon.settings.client.read_local_logs = read_local_logs;
    // Persist the change, so it can be seen by the client.
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__default", output, context)?;

    Ok(())
}

/// Test that the `log` command properly truncates content and hints this to the user for:
/// - The log being streamed by the daemon.
/// - The log being read from the local files.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_truncated(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Force the client to read remote logs via config file.
    daemon.settings.client.read_local_logs = read_local_logs;
    // Persist the change, so it can be seen by the client.
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))
        .context("Couldn't write pueue config to temporary directory")?;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo '1\n2\n3\n4\n5\n6\n7\n8\n9\n10'").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log", "--lines=5"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__last_lines", output, context)?;

    Ok(())
}

/// Byte limits return exactly the suffix and report whether it is incomplete.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn byte_limit(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    daemon.settings.client.read_local_logs = read_local_logs;
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))?;

    add_task(shared, "printf abcdef").await?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;
    let output = run_client_command(shared, &["log", "--json", "--max-bytes=3", "0"])?.success()?;
    let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(task_logs[&0].output, "def");
    assert!(!task_logs[&0].output_complete);

    let output = run_client_command(shared, &["log", "--json", "--head", "--max-bytes=3", "0"])?
        .success()?;
    let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(task_logs[&0].output, "abc");
    assert!(!task_logs[&0].output_complete);

    add_task(shared, "printf 'one\\ntwo\\nthree\\n'").await?;
    wait_for_task_condition(shared, 1, Task::is_done).await?;
    let output =
        run_client_command(shared, &["log", "--json", "--head", "--lines=2", "1"])?.success()?;
    let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(task_logs[&1].output, "one\ntwo\n");
    assert!(!task_logs[&1].output_complete);

    let output =
        run_client_command(shared, &["log", "--json", "--max-bytes=100", "0"])?.success()?;
    let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(task_logs[&0].output, "abcdef");
    assert!(task_logs[&0].output_complete);

    Ok(())
}

/// Tail selection handles zero limits, unterminated lines, CRLF, and exact boundaries.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tail_line_boundaries(#[case] read_local_logs: bool) -> Result<()> {
    let mut daemon = daemon().await?;
    let shared = &daemon.settings.shared;
    daemon.settings.client.read_local_logs = read_local_logs;
    daemon
        .settings
        .save(&Some(daemon.tempdir.path().join("pueue.yml")))?;

    let cases = [
        ("printf 'one\\ntwo'", 1, "two", false),
        ("printf 'one\\ntwo'", 0, "", false),
        ("printf lone", 0, "", false),
        ("printf 'one\\r\\ntwo\\r\\n'", 1, "two\r\n", false),
        ("printf 'one\\ntwo\\n'", 2, "one\ntwo\n", true),
        ("printf ''", 0, "", true),
    ];
    for (id, (command, lines, expected, complete)) in cases.into_iter().enumerate() {
        add_task(shared, command).await?;
        wait_for_task_condition(shared, id, Task::is_done).await?;
        let lines = format!("--lines={lines}");
        let id_string = id.to_string();
        let output = run_client_command(
            shared,
            &["log", "--json", lines.as_str(), id_string.as_str()],
        )?
        .success()?;
        let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_slice(&output.stdout)?;
        assert_eq!(task_logs[&id].output, expected, "command: {command}");
        assert_eq!(task_logs[&id].output_complete, complete);
    }

    Ok(())
}

/// If a task has a label, it is included in the log output
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_with_label() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it finishes.
    run_client_command(shared, &["add", "--label", "test_label", "echo test"])?;
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__with_label", output, context)?;

    Ok(())
}

/// Calling `log` with the `--color=always` flag, colors the output as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn colored() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["--color", "always", "log"])?;

    let context = get_task_context(&daemon.settings).await?;
    assert_template_matches("log__colored", output, context)?;

    Ok(())
}

/// Tasks that have not started have an explanatory message rather than a file error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn never_run_task() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    run_client_command(shared, &["add", "--stashed", "echo test"])?;
    let output = run_client_command(shared, &["log", "0"])?;
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Task has not run; no log output is available."));
    assert!(!stderr.contains("I/O error"));

    let output = run_client_command(shared, &["log", "--json", "0"])?;
    assert!(output.status.success());
    let json = String::from_utf8_lossy(&output.stdout);
    let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_str(&json)?;
    assert_eq!(task_logs[&0].output, "");

    Ok(())
}

/// A dependency-failed task has no process and therefore no log file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_failed_task() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    run_client_command(shared, &["add", "failing_command"])?;
    run_client_command(shared, &["add", "--after=0", "echo unreachable"])?;
    wait_for_task_condition(shared, 1, Task::is_done).await?;

    let output = run_client_command(shared, &["log", "1"])?;
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Task has not run; no log output is available."));
    assert!(!stderr.contains("I/O error"));

    Ok(())
}

/// Failed-to-spawn diagnostics are written to a log and remain available through both read paths.
#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_to_spawn_task(#[case] read_local_logs: bool) -> Result<()> {
    let (mut settings, tempdir) = daemon_base_setup()?;
    settings.daemon.shell_command = Some(vec!["thisshellshouldreallynotexist.hopefully".into()]);
    settings.client.read_local_logs = read_local_logs;
    let config_path = tempdir.path().join("pueue.yml");
    settings.save(&Some(config_path))?;
    let daemon = daemon_with_settings(settings, tempdir).await?;
    let shared = &daemon.settings.shared;

    add_task(shared, "echo test").await?;
    wait_for_task_condition(shared, 0, |task| task.failed()).await?;

    let output = run_client_command(shared, &["log", "0"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Pueue error, failed to spawn task. Check your command."));
    assert!(stdout.contains("using shell `thisshellshouldreallynotexist.hopefully`"));
    assert!(stdout.contains("for command `echo test`"));

    let output = run_client_command(shared, &["log", "--json", "0"])?;
    let json = String::from_utf8_lossy(&output.stdout);
    let task_logs: BTreeMap<usize, TaskLog> = serde_json::from_str(&json)?;
    assert!(
        task_logs[&0]
            .output
            .contains("Pueue error, failed to spawn task. Check your command.")
    );

    Ok(())
}

/// This is the output struct used for task logs.
/// Since the Pueue client isn't exposed as a library, we have to declare our own for testing
/// purposes. The counter part can be found in `client/display/log/json.rs`.
#[derive(Debug, Deserialize)]
pub struct TaskLog {
    pub task: Task,
    pub output: String,
    pub output_complete: bool,
}

/// Calling `pueue log --json` prints the expected json output to stdout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn json() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a task and wait until it finishes.
    assert_success(add_task(shared, "echo test").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let output = run_client_command(shared, &["log", "--json"])?;

    // Deserialize the json back to the original task BTreeMap.
    let json = String::from_utf8_lossy(&output.stdout);
    let mut task_logs: BTreeMap<usize, TaskLog> = serde_json::from_str(&json)
        .context(format!("Failed to deserialize json tasks: \n{json}"))?;

    // Get the actual BTreeMap from the daemon
    let mut state = get_state(shared).await?;
    let original_task = state.tasks.get_mut(&0).unwrap();
    // Clean the environment variables, as they aren't transmitted when calling `log`.
    original_task.envs = HashMap::new();

    let task_log = task_logs.get_mut(&0).expect("Expected one task log");
    assert_eq!(
        original_task, &task_log.task,
        "Deserialized task and original task aren't equal"
    );

    // Preserve the log's trailing newline in JSON output.
    assert_eq!("test\n", task_log.output);

    Ok(())
}
