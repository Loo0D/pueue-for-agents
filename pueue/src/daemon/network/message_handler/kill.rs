use pueue_lib::{Settings, Task, TaskStatus, message::*, success_msg};

use super::ok_or_failure_message;
use crate::daemon::{
    internal_state::SharedState,
    network::response_helper::{
        compile_task_response, ensure_group_exists, task_action_response_helper,
    },
    process_handler,
};
use crate::ok_or_save_state_failure;

/// Invoked when calling `pueue kill`.
/// Forward the kill message to the task handler, which then kills the process.
pub fn kill(settings: &Settings, state: &SharedState, message: KillRequest) -> Response {
    let mut state = state.lock().unwrap();

    // If a group is selected, make sure it exists.
    if let TaskSelection::Group(group) = &message.tasks
        && let Err(response) = ensure_group_exists(&mut state, group)
    {
        return response;
    }

    // For explicitly selected tasks, killing pending work means removing it from the queue.
    // Unix signals still only make sense for tasks with a live process.
    if message.signal.is_none()
        && let TaskSelection::TaskIds(task_ids) = &message.tasks
    {
        let queued_ids = state
            .filter_tasks(
                |task| {
                    matches!(
                        task.status,
                        TaskStatus::Queued { .. } | TaskStatus::Stashed { .. }
                    )
                },
                Some(task_ids.clone()),
            )
            .matching_ids;

        if !queued_ids.is_empty() {
            let mut filtered_tasks = state.filter_tasks(
                |task| {
                    task.is_running()
                        || matches!(
                            task.status,
                            TaskStatus::Queued { .. } | TaskStatus::Stashed { .. }
                        )
                },
                Some(task_ids.clone()),
            );
            let running_ids = state
                .filter_tasks(Task::is_running, Some(task_ids.clone()))
                .matching_ids;

            if !running_ids.is_empty() {
                process_handler::kill::kill(
                    settings,
                    &mut state,
                    TaskSelection::TaskIds(running_ids.clone()),
                    true,
                    None,
                );
            }

            let removed =
                super::remove::remove_tasks(settings, &mut state, queued_ids, &running_ids);
            filtered_tasks.matching_ids = [running_ids, removed.matching_ids].concat();
            filtered_tasks
                .non_matching_ids
                .extend(removed.non_matching_ids);
            ok_or_save_state_failure!(state.save(settings));

            return compile_task_response("Tasks killed or removed from list", filtered_tasks);
        }
    }

    // Construct a response depending on the selected tasks.
    let response = if let Some(signal) = &message.signal {
        match &message.tasks {
            TaskSelection::TaskIds(task_ids) => task_action_response_helper(
                "Tasks are being killed",
                task_ids.clone(),
                Task::is_running,
                &state,
            ),
            TaskSelection::Group(group) => {
                success_msg!("Sending signal {signal} to all running tasks of group {group}.",)
            }
            TaskSelection::All => {
                success_msg!("Sending signal {signal} to all running tasks.")
            }
        }
    } else {
        match &message.tasks {
            TaskSelection::TaskIds(task_ids) => task_action_response_helper(
                "Tasks are being killed",
                task_ids.clone(),
                Task::is_running,
                &state,
            ),
            TaskSelection::Group(group) => success_msg!(
                "All tasks of group \"{group}\" are being killed. The group will also be paused!!!"
            ),
            TaskSelection::All => {
                success_msg!("All tasks are being killed. All groups will be paused!!!")
            }
        }
    };

    // Actually execute the command
    if matches!(
        &response,
        Response::Success(_) | Response::PartialFailure(_)
    ) {
        process_handler::kill::kill(settings, &mut state, message.tasks, true, message.signal);
    }

    response
}
