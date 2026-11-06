use std::{collections::BTreeMap, io::Read};

use pueue_lib::{
    log::{get_log_file_handle, seek_to_log_bounds},
    message::TaskLogResponse,
    settings::Settings,
    task::Task,
};
use serde::{Deserialize, Serialize};
use snap::read::FrameDecoder;

/// This is the output struct used for
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskLog {
    pub task: Task,
    pub output: String,
    pub output_complete: bool,
}

/// Print some log output in JSON serialized form.
///
/// If the log isn't read from the disk but rather received from the daemon, we have to
/// convert the received [TaskLogResponse] into a proper JSON serializable format.
/// Output in [TaskLogResponse], is usually compressed, so we need to decompress it first.
pub fn print_log_json(
    task_log_messages: BTreeMap<usize, TaskLogResponse>,
    settings: &Settings,
    lines: Option<usize>,
    max_bytes: Option<usize>,
    head: bool,
) {
    let mut json = BTreeMap::new();
    for (id, message) in task_log_messages {
        let has_process_log = message.task.has_process_log();
        let mut task = message.task;
        let (output, output_complete) = if !has_process_log {
            (String::new(), true)
        } else if settings.client.read_local_logs {
            get_local_log(settings, id, lines, max_bytes, head)
        } else {
            (get_remote_log(message.output), message.output_complete)
        };

        task.envs.clear();
        json.insert(
            id,
            TaskLog {
                task,
                output,
                output_complete,
            },
        );
    }

    println!("{}", serde_json::to_string(&json).unwrap());
}

/// Read logs directly from local files for a specific task.
fn get_local_log(
    settings: &Settings,
    id: usize,
    lines: Option<usize>,
    max_bytes: Option<usize>,
    head: bool,
) -> (String, bool) {
    let mut file = match get_log_file_handle(id, &settings.shared.pueue_directory()) {
        Ok(file) => file,
        Err(err) => {
            return (
                format!("(Pueue error) Failed to get log file handle: {err}"),
                false,
            );
        }
    };
    let bounds = match seek_to_log_bounds(&mut file, lines, max_bytes, head) {
        Ok(bounds) => bounds,
        Err(error) => {
            return (
                format!("(Pueue error) Failed to read local log output file: {error}"),
                false,
            );
        }
    };

    let output_complete = bounds.output_complete;
    let mut reader = file.take(bounds.read_limit.unwrap_or(u64::MAX));
    let mut output = Vec::new();
    if let Err(error) = reader.read_to_end(&mut output) {
        return (
            format!("(Pueue error) Failed to read local log output file: {error}"),
            false,
        );
    }

    (
        String::from_utf8_lossy(&output).into_owned(),
        output_complete,
    )
}

/// Read logs from from compressed remote logs.
/// If logs don't exist, an empty string will be returned.
fn get_remote_log(output_bytes: Option<Vec<u8>>) -> String {
    let Some(bytes) = output_bytes else {
        return String::new();
    };

    let mut decoder = FrameDecoder::new(&bytes[..]);
    let mut output = Vec::new();
    if let Err(error) = decoder.read_to_end(&mut output) {
        return format!("(Pueue error) Failed to decompress remote log output: {error}");
    }

    String::from_utf8_lossy(&output).into_owned()
}
