use pueue_lib::{Task, message::*};

use crate::{helper::*, internal_prelude::*};

/// Add and directly remove a group.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_add_and_remove() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    // Add a new group
    add_group_with_slots(shared, "testgroup", 1).await?;

    // Try to add the same group again. This should fail
    let add_message = GroupRequest::Add {
        name: "testgroup".to_string(),
        parallel_tasks: None,
    };
    assert_failure(send_request(shared, add_message).await?);

    // Remove the newly added group and wait for the deletion to be processed.
    let remove_message = GroupRequest::Remove("testgroup".to_string());
    assert_success(send_request(shared, remove_message.clone()).await?);
    wait_for_group_absence(shared, "testgroup").await?;

    // Make sure it got removed
    let state = get_state(shared).await?;
    assert!(!state.groups.contains_key("testgroup"));

    Ok(())
}

/// Users cannot delete the default group.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cannot_delete_default() -> Result<()> {
    let daemon = daemon().await?;

    let message = GroupRequest::Remove(PUEUE_DEFAULT_GROUP.to_string());
    assert_failure(send_request(&daemon.settings.shared, message).await?);

    Ok(())
}

/// Users cannot delete a non-existing group.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cannot_delete_non_existing() -> Result<()> {
    let daemon = daemon().await?;

    let message = GroupRequest::Remove("doesnt_exist".to_string());
    assert_failure(send_request(&daemon.settings.shared, message).await?);

    Ok(())
}

/// Removing a group moves inactive tasks to the default group.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_moves_tasks_to_default() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    add_group_with_slots(shared, "testgroup", 1).await?;
    assert_success(add_task_to_group(shared, "ls", "testgroup").await?);
    wait_for_task_condition(shared, 0, Task::is_done).await?;

    let mut message = create_add_message(shared, "echo next");
    message.group = "testgroup".into();
    message.stashed = true;
    assert_success(send_request(shared, message).await?);

    let message = GroupRequest::Remove("testgroup".to_string());
    assert_success(send_request(shared, message).await?);

    let state = get_state(shared).await?;
    assert!(!state.groups.contains_key("testgroup"));
    assert_eq!(state.tasks.len(), 2);
    assert!(state.tasks.values().all(|task| task.group == "default"));

    Ok(())
}

/// A running task still needs its group's worker pool and cannot be moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cannot_delete_group_with_running_task() -> Result<()> {
    let daemon = daemon().await?;
    let shared = &daemon.settings.shared;

    add_group_with_slots(shared, "testgroup", 1).await?;
    let mut message = create_add_message(shared, "sleep 60");
    message.group = "testgroup".into();
    message.start_immediately = true;
    assert_success(send_request(shared, message).await?);
    wait_for_task_condition(shared, 0, Task::is_running).await?;

    let message = GroupRequest::Remove("testgroup".to_string());
    assert_failure(send_request(shared, message).await?);
    assert_eq!(get_state(shared).await?.tasks[&0].group, "testgroup");

    Ok(())
}
