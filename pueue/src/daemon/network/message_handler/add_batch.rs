use std::collections::{BTreeMap, BTreeSet};

use chrono::Local;
use pueue_lib::{Settings, Task, TaskStatus, failure_msg, message::*};

use crate::{aliasing::insert_alias, daemon::internal_state::SharedState};

pub fn add_batch(settings: &Settings, state: &SharedState, message: AddBatchRequest) -> Response {
    if message.tasks.is_empty() {
        return failure_msg!("A batch must contain at least one task.");
    }

    let mut state = state.lock().unwrap();
    let mut task_indices = BTreeMap::new();
    for (index, task) in message.tasks.iter().enumerate() {
        if task_indices.insert(task.name.clone(), index).is_some() {
            return failure_msg!("Duplicate task name: {:?}", task.name);
        }
        if !state.groups().contains_key(&task.group) {
            return failure_msg!("Group {:?} doesn't exist.", task.group);
        }
    }

    for task in &message.tasks {
        for dependency in &task.dependencies {
            if !task_indices.contains_key(dependency) {
                return failure_msg!(
                    "Unknown dependency {dependency:?} for task {:?}.",
                    task.name
                );
            }
        }
    }

    let mut order = Vec::new();
    let mut ordered = BTreeSet::new();
    let mut resolved = BTreeSet::new();
    while order.len() < message.tasks.len() {
        let previous_len = order.len();
        for task in &message.tasks {
            if ordered.contains(task.name.as_str()) {
                continue;
            }
            if task
                .dependencies
                .iter()
                .all(|dependency| resolved.contains(dependency.as_str()))
            {
                order.push(task_indices[&task.name]);
                ordered.insert(task.name.as_str());
                resolved.insert(task.name.as_str());
            }
        }
        if order.len() == previous_len {
            return failure_msg!("The batch contains a dependency cycle.");
        }
    }

    let mut task_ids = BTreeMap::new();
    for index in order {
        let task = &message.tasks[index];
        let dependencies = task
            .dependencies
            .iter()
            .map(|name| task_ids[name])
            .collect();
        let status = if task.stashed {
            TaskStatus::Stashed { enqueue_at: None }
        } else {
            TaskStatus::Queued {
                enqueued_at: Local::now(),
            }
        };
        let mut new_task = Task::new(
            task.command.clone(),
            task.path.clone(),
            message.envs.clone(),
            task.group.clone(),
            status,
            dependencies,
            task.priority.unwrap_or(0),
            task.label.clone(),
        );
        new_task.command = insert_alias(settings, new_task.original_command.clone());
        new_task.dependencies.sort_unstable();
        new_task.dependencies.dedup();

        let task_id = state.add_task(new_task);
        task_ids.insert(task.name.clone(), task_id);
    }

    if let Err(error) = state.save(settings) {
        for task_id in task_ids.values() {
            state.tasks_mut().remove(task_id);
        }
        return failure_msg!("Failed to save state. This is a bug: {error}");
    }

    AddedBatchResponse { task_ids }.into()
}
