use std::{
    fs::File,
    io::{self, Read, Stdout},
};

use crossterm::style::{Attribute, Color};
use pueue_lib::{
    log::{get_log_file_handle, seek_to_log_bounds},
    settings::Settings,
};

use crate::client::style::OutputStyle;

/// The daemon didn't send any log output, thereby we didn't request any.
/// If that's the case, read the log file from the local pueue directory.
pub fn print_local_log(
    task_id: usize,
    style: &OutputStyle,
    settings: &Settings,
    lines: Option<usize>,
    max_bytes: Option<usize>,
    head: bool,
) {
    let mut file = match get_log_file_handle(task_id, &settings.shared.pueue_directory()) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Failed to get log file handle: {err}");
            return;
        }
    };
    // Stdout handler to directly write log file output to io::stdout
    // without having to load anything into memory.
    let mut stdout = io::stdout();

    print_local_file(
        &mut stdout,
        &mut file,
        &lines,
        &max_bytes,
        &head,
        style.style_text("output:", Some(Color::Green), Some(Attribute::Bold)),
    );
}

/// Print a local log file of a task.
fn print_local_file(
    stdout: &mut Stdout,
    file: &mut File,
    lines: &Option<usize>,
    max_bytes: &Option<usize>,
    head: &bool,
    header: String,
) {
    if let Ok(metadata) = file.metadata()
        && metadata.len() != 0
    {
        // Seek before copying so the discarded portion never needs to be read.
        let bounds = match seek_to_log_bounds(file, *lines, *max_bytes, *head) {
            Ok(bounds) => bounds,
            Err(err) => {
                eprintln!("Failed reading local log file: {err}");
                return;
            }
        };
        let output_complete = bounds.output_complete;

        // Add a hint if we should limit the output to X lines **and** there are actually more
        // lines than that given limit.
        let mut line_info = String::new();
        if !output_complete {
            let direction = if *head { "first" } else { "last" };
            let mut limits = Vec::new();
            if let Some(lines) = lines {
                limits.push(format!("{direction} {lines} lines"));
            }
            if let Some(max_bytes) = max_bytes {
                limits.push(format!("{direction} {max_bytes} bytes"));
            }
            line_info = format!(" ({})", limits.join(", "));
        }

        // Print a newline between the task information and the first output.
        eprintln!("\n{header}{line_info}");

        // Print the selected portion.
        let mut reader = file.take(bounds.read_limit.unwrap_or(u64::MAX));
        if let Err(err) = io::copy(&mut reader, stdout) {
            eprintln!("Failed reading local log file: {err}");
        };
    }
}
