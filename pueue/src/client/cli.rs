use std::{ffi::OsString, path::PathBuf};

use chrono::{TimeDelta, prelude::*};
use clap::{ArgAction, CommandFactory, Parser, ValueEnum, ValueHint};
use interim::*;
use pueue_lib::message::Signal;

use crate::client::commands::WaitTargetStatus;

#[derive(Parser, Debug, Clone)]
pub enum SubCommand {
    /// Enqueue a task for execution.
    ///
    /// There're many different options when scheduling a task.
    /// Check the individual option help texts for more information.
    /// Furthermore, please remember that scheduled commands are executed via your system shell.
    /// This means that the command needs proper shell escaping.
    /// The safest way to preserve shell escaping is to surround your command with quotes, for
    /// example:
    ///
    /// pueue add 'ls $HOME && echo \"Some string\"'
    #[command(trailing_var_arg = true)]
    Add {
        /// The command to be added.
        #[arg(required = true, num_args(1..), value_hint = ValueHint::CommandWithArguments)]
        command: Vec<String>,

        /// Specify current working directory.
        #[arg(name = "working-directory", short = 'w', long, value_hint = ValueHint::DirPath)]
        working_directory: Option<PathBuf>,

        /// Escape any special shell characters (" ", "&", "!", etc.).
        /// Beware: This implicitly disables nearly all shell specific syntax ("&&", "&>").
        #[arg(verbatim_doc_comment, short, long)]
        escape: bool,

        /// Immediately start the task.
        #[arg(name = "immediate", short, long, conflicts_with = "stashed")]
        start_immediately: bool,

        /// Immediately follow a task, if it's started with --immediate.
        #[arg(
            name = "follow",
            long,
            requires = "immediate",
            conflicts_with = "print_task_id"
        )]
        follow: bool,

        /// Create the task in Stashed state.
        ///
        /// Useful to avoid immediate execution if the queue is empty.
        #[arg(short, long, conflicts_with = "immediate")]
        stashed: bool,

        /// Prevents the task from being enqueued until 'delay' elapses. See "enqueue" for accepted
        /// formats.
        #[arg(name = "delay", short, long, conflicts_with = "immediate", value_parser = parse_delay_until)]
        delay_until: Option<DateTime<Local>>,

        /// Assign the task to a group.
        ///
        /// Groups kind of act as separate queues.
        /// All groups run in parallel and you can specify the amount of parallel tasks for each
        /// group. If no group is specified, the default group will be used.
        ///
        /// Create groups via the `pueue group` subcommand
        #[arg(short, long)]
        group: Option<String>,

        /// Start the task once all specified tasks have successfully finished.
        /// Repeat this option or separate multiple task ids with commas.
        ///
        /// As soon as one of the dependencies fails, this task will fail as well.
        #[arg(name = "after", short, long, value_delimiter = ',')]
        dependencies: Vec<usize>,

        /// Start this task with a higher priority.
        ///
        /// The higher the number, the faster it will be processed.
        #[arg(short = 'o', long)]
        priority: Option<i32>,

        /// Add some information for yourself.
        ///
        /// This string will be shown in the "status" table.
        /// There's no additional logic connected to it.
        #[arg(short, long)]
        label: Option<String>,

        /// Only return the task id instead of a text.
        ///
        /// This is useful when working with dependencies in scripts.
        #[arg(short, long)]
        print_task_id: bool,

        /// Inherit one environment variable by key or set an explicit KEY=VALUE pair.
        /// Can be repeated.
        #[arg(long = "env", value_name = "KEY[=VALUE]")]
        environment: Vec<String>,

        /// Inherit the complete client environment, including possible credentials.
        #[arg(long)]
        inherit_env: bool,
    },
    /// Atomically add a named dependency graph from JSON.
    /// Reads stdin when no file is supplied or when the file is `-`.
    AddBatch {
        /// JSON document containing a top-level `tasks` array.
        file: Option<PathBuf>,

        /// Inherit or set an environment variable shared by every batch task.
        #[arg(long = "env", value_name = "KEY[=VALUE]")]
        environment: Vec<String>,

        /// Inherit the complete client environment for every batch task.
        #[arg(long)]
        inherit_env: bool,
    },
    /// Remove tasks from the list.
    /// Running or paused tasks need to be killed first.
    #[command(alias("rm"))]
    Remove {
        /// The task ids to be removed.
        #[arg(required = true)]
        task_ids: Vec<usize>,
    },
    /// Switches the queue position of two commands.
    ///
    /// Only works on queued and stashed commands.
    Switch {
        /// The first task id.
        task_id_1: usize,
        /// The second task id.
        task_id_2: usize,
    },
    /// Stash a task. Stashed tasks won't be automatically started.
    ///
    /// The enqueue an item, use the `pueue enqueue` subcommand.
    /// Stashed entries can also be explicitely started via `pueue start $task_id`.
    Stash {
        /// Stash these specific tasks.
        task_ids: Vec<usize>,

        /// Stash all queued tasks in a group
        #[arg(short, long, conflicts_with = "all")]
        group: Option<String>,

        /// Stash all queued tasks across all groups.
        #[arg(short, long)]
        all: bool,

        /// Delay enqueuing these tasks until 'delay' elapses. See DELAY FORMAT below.
        #[arg(name = "delay", short, long, value_parser = parse_delay_until)]
        delay_until: Option<DateTime<Local>>,
    },
    /// Enqueue stashed tasks. They'll be handled normally afterwards.
    ///
    /// Enqueues all stashed task in the default group if no arguments are given.
    #[command(after_help = "DELAY FORMAT:

    The --delay argument must be either a number of seconds or a \"date expression\" similar to GNU \
    \"date -d\" with some extensions. It does not attempt to parse all natural language, but is \
    incredibly flexible. Here are some supported examples.

    2020-04-01T18:30:00   // RFC 3339 timestamp
    2020-4-1 18:2:30      // Optional leading zeros
    2020-4-1 5:30pm       // Informal am/pm time
    2020-4-1 5pm          // Optional minutes and seconds
    April 1 2020 18:30:00 // English months
    1 Apr 8:30pm          // Implies current year
    4/1                   // American form date
    wednesday 10:30pm     // The closest wednesday in the future at 22:30
    wednesday             // The closest wednesday in the future
    4 months              // 4 months from today at 00:00:00
    1 week                // 1 week at the current time
    1days                 // 1 day from today at the current time
    1d 03:00              // The closest 3:00 after 1 day (24 hours)
    3h                    // 3 hours from now
    3600s                 // 3600 seconds from now
")]
    Enqueue {
        /// Enqueue these specific tasks.
        task_ids: Vec<usize>,

        /// Enqueue all stashed tasks in a group
        #[arg(short, long, conflicts_with = "all")]
        group: Option<String>,

        /// Enqueue all stashed tasks across all groups.
        #[arg(short, long)]
        all: bool,

        /// Delay enqueuing these tasks until 'delay' elapses. See DELAY FORMAT below.
        #[arg(name = "delay", short, long, value_parser = parse_delay_until)]
        delay_until: Option<DateTime<Local>>,
    },

    /// Resume operation of specific tasks or groups of tasks.
    ///
    /// Without any parameters this resumes the default group and all its tasks.
    /// Can also be used force-start specific tasks, which **ignores** any group parallelism limits
    /// or dependencies a task my have!
    Start {
        /// Start these specific tasks. Paused tasks will resumed. Queued/Stashed tasks will be
        /// force-started.
        task_ids: Vec<usize>,

        /// Resume a specific group and all paused tasks in it.
        ///
        /// The group will be set to running and its paused tasks will be resumed.
        #[arg(short, long, conflicts_with = "all")]
        group: Option<String>,

        /// Resume all groups!
        ///
        /// All groups will be set to running and paused tasks will be resumed.
        #[arg(short, long)]
        all: bool,
    },

    /// Restart failed or successful task(s).
    ///
    /// By default, identical tasks will be created and enqueued, but it's possible to restart
    /// in-place. You can also edit a few properties, such as the path and the command, before
    /// restarting.
    #[command(alias("re"))]
    Restart {
        /// Restart these specific tasks.
        task_ids: Vec<usize>,

        /// Restart all failed tasks across all groups.
        ///
        /// This is nice for usage in combination with `-i/--in-place` (or the respective config
        /// option).
        #[arg(short, long)]
        all_failed: bool,

        /// Like `--all-failed`, but only restart tasks failed tasks of a specific group.
        #[arg(short = 'g', long, conflicts_with = "all_failed")]
        failed_in_group: Option<String>,

        /// Immediately start the tasks, no matter how many open slots there are.
        /// This will ignore any dependencies tasks may have.
        #[arg(short = 'k', long = "immediate", conflicts_with = "stashed")]
        start_immediately: bool,

        /// Set the restarted task to a "Stashed" state.
        /// Useful to avoid immediate execution.
        #[arg(short, long)]
        stashed: bool,

        /// Restart the task by reusing the already existing tasks.
        /// This will overwrite any previous logs of the restarted tasks.
        ///
        /// This can also be enabled by default via the `restart_in_place` config option.
        #[arg(short, long)]
        in_place: bool,

        /// Restart the task by creating a new identical tasks.
        /// Only necessary if you have the `restart_in_place` configuration set to true.
        #[arg(long)]
        not_in_place: bool,

        /// Edit the task before restarting.
        #[arg(short, long)]
        edit: bool,

        /// Discard dependencies when creating replacement tasks.
        #[arg(long)]
        drop_dependencies: bool,

        /// Restart all transitive dependents and remap dependencies to replacement tasks.
        #[arg(long)]
        with_dependents: bool,

        /// Print only the resulting task ids, one per line.
        #[arg(short = 'p', long)]
        print_task_id: bool,
    },

    /// Either pause running tasks or specific groups of tasks.
    ///
    /// By default, pauses the default group and all its tasks.
    /// A paused queue (group) won't start any new tasks.
    Pause {
        /// Pause these specific tasks.
        task_ids: Vec<usize>,

        /// Pause a specific group.
        #[arg(short, long, conflicts_with = "all")]
        group: Option<String>,

        /// Pause all groups!
        #[arg(short, long)]
        all: bool,

        /// Pause the specified groups, but let already running tasks finish by themselves.
        #[arg(short, long)]
        wait: bool,
    },

    /// Kill specific running tasks or whole task groups.
    ///
    /// Kills all tasks of the default group when no ids or a specific group are provided.
    Kill {
        /// Kill these specific tasks.
        task_ids: Vec<usize>,

        /// Kill all running tasks in a group. This also pauses the group.
        #[arg(short, long, conflicts_with = "all")]
        group: Option<String>,

        /// Kill all running tasks across ALL groups. This also pauses all groups.
        #[arg(short, long)]
        all: bool,

        /// Send a UNIX signal instead of simply killing the process.
        ///
        /// DISCLAIMER: This bypasses Pueue's process handling logic!
        ///     You might enter weird invalid states, use at your own descretion.
        ///
        /// This argument also excepts the integer representation as well as the signal
        /// short name. E.g. `sigint`, `int`, or `2` are the same.
        #[arg(short, long, ignore_case(true))]
        signal: Option<Signal>,
    },

    /// Send something to a task. Useful for sending confirmations such as 'y\n'.
    Send {
        /// The id of the task.
        task_id: usize,

        /// The input that should be sent to the process.
        input: String,
    },

    /// Adjust editable properties of a task.
    ///
    /// Only stashed or queued tasks can be edited.
    /// A temporary folder folder/file will be opened by your $EDITOR to edit the tasks.
    Edit {
        /// The ids of all tasks that should be edited.
        task_ids: Vec<usize>,
    },

    /// Use this to add or remove environment variables from tasks.
    Env {
        #[command(subcommand)]
        cmd: EnvCommand,
    },

    /// Use this to add or remove groups.
    ///
    /// By default, this will simply display all known groups.
    Group {
        /// Print the list of groups as json.
        #[arg(short, long)]
        json: bool,

        #[command(subcommand)]
        cmd: Option<GroupCommand>,
    },

    /// Display selected tasks as JSON without returning the rest of the queue.
    Show {
        /// The task ids to display.
        #[arg(required = true)]
        task_ids: Vec<usize>,

        /// Include task environments, which may contain secrets.
        #[arg(long)]
        include_env: bool,
    },

    /// Display the current status of all tasks.
    Status {
        /// Users can specify a custom query to filter for specific values, order by a column
        /// or limit the amount of tasks listed.
        /// Use `--help` for the full syntax definition.
        #[arg(
            long_help = "Users can specify a custom query to filter for specific values, order by a column
or limit the amount of tasks listed.

Syntax:
   [column_selection]? [filter]* [order_by]? [limit]?

where:
  - column_selection := `columns=[column]([column],)*`
  - column := `id | status | command | label | path | enqueue_at | dependencies | start | end`
  - filter := `[filter_column] [filter_op] [filter_value]`
    (note: not all columns support all operators, see \"Filter columns\" below.)
  - filter_column := `status | command | label | start | end | enqueue_at`
  - filter_op := `= | != | < | > | %=`
    (`%=` means 'contains', as in the test value is a substring of the column value)
  - order_by := `order_by [column] [order_direction]`
  - order_direction := `asc | desc`
  - limit := `[limit_type]? [limit_count]`
  - limit_type := `first | last`
  - limit_count := a positive integer

Filter columns:
  - `status` supports the operators `=`, `!=`
    against test values that are:
      - strings like `queued`, `stashed`, `paused`, `running`, `success`, `failed`
  - `command`, `label` support the operators `=`, `!=`, `%=`
    against test values that are:
      - strings like `some text`
  - `start`, `end`, `enqueue_at` contain a datetime
    which support the operators `=`, `!=`, `<`, `>`
    against test values that are:
      - date like `YYYY-MM-DD`
      - time like `HH:mm:ss` or `HH:mm`
      - datetime like `YYYY-MM-DDHH:mm:ss`
        (note there is currently no separator between the date and the time)

Examples:
  - `status=running`
  - `command%=echo`
  - `label=mytask`
  - `columns=id,status,command status=running start > 2023-05-2112:03:17 order_by command first 5`

The formal syntax is defined here:
https://github.com/Nukesor/pueue/blob/main/pueue/src/client/commands/state/query/syntax.pest

More documentation is on the query syntax PR:
https://github.com/Nukesor/pueue/issues/350#issue-1359083118"
        )]
        query: Vec<String>,

        /// Print the current state as json to stdout.
        /// This does not include the output of tasks or task environments.
        /// Use `log -j` if you want task output as well.
        #[arg(short, long)]
        json: bool,

        /// Include task environments in JSON output.
        /// Environments often contain secrets, so they are omitted by default.
        #[arg(long, requires = "json")]
        include_env: bool,

        #[arg(short, long)]
        /// Only show tasks of a specific group
        group: Option<String>,
    },

    /// Display the log output of finished tasks.
    ///
    /// Only the last few lines will be shown by default.
    /// If you want to follow the output of a task, please use the \"follow\" subcommand.
    Log {
        /// View the task output of these specific tasks.
        task_ids: Vec<usize>,

        /// View the outputs of this specific group's tasks.
        #[arg(short, long)]
        group: Option<String>,

        /// Show the logs of all groups' tasks.
        #[arg(short, long)]
        all: bool,

        /// Print the resulting tasks and output as json.
        ///
        /// By default only the last lines will be returned unless --full is provided.
        /// Take care, as the json cannot be streamed!
        /// If your logs are really huge, using --full can use all of your machine's RAM.
        #[arg(short, long)]
        json: bool,

        /// Only print the last X lines of each task's output.
        ///
        /// This is done by default if you're looking at multiple tasks.
        #[arg(short, long, conflicts_with = "full")]
        lines: Option<usize>,

        /// Limit output to the last N bytes, or the first N bytes with `--head`.
        #[arg(long, conflicts_with = "full")]
        max_bytes: Option<usize>,

        /// Select the beginning instead of the end when using `--lines` or `--max-bytes`.
        #[arg(long, conflicts_with = "full")]
        head: bool,

        /// Show the whole output.
        #[arg(short, long)]
        full: bool,
    },

    /// Follow the output of a currently running task.
    /// This command works like "tail -f".
    #[command(alias("fo"))]
    Follow {
        /// The id of the task you want to watch.
        ///
        /// If no or multiple tasks are running, you have to specify the id.
        /// If only a single task is running, you can omit the id.
        task_id: Option<usize>,

        /// Only print the last X lines of the output before following
        #[arg(short, long)]
        lines: Option<usize>,
    },

    /// Wait until tasks are finished.
    ///
    /// By default, this will wait for all tasks in the default group to finish.
    ///
    /// Note: This will also wait for all tasks that aren't somehow 'Done'.
    /// Includes: [Paused, Stashed, Locked, Queued, ...]
    Wait {
        /// This allows you to wait for specific tasks to finish.
        task_ids: Vec<usize>,

        /// Wait for all tasks in a specific group
        #[arg(short, long, conflicts_with = "all")]
        group: Option<String>,

        /// Wait for all tasks across all groups and the default group.
        #[arg(short, long)]
        all: bool,

        /// Don't show any log output while waiting
        #[arg(short, long)]
        quiet: bool,

        /// Wait for tasks to reach a specific task status.
        #[arg(short, long)]
        status: Option<WaitTargetStatus>,

        /// Emit a final machine-readable result document instead of progress messages.
        #[arg(long)]
        json: bool,
    },

    /// Remove all finished tasks from the list.
    #[command(aliases(["cleanup", "clear"]))]
    Clean {
        /// Only clean tasks that finished successfully.
        #[arg(short, long)]
        successful_only: bool,

        /// Only clean tasks of a specific group
        #[arg(short, long)]
        group: Option<String>,
    },

    /// Kill all tasks, clean up afterwards and reset EVERYTHING!
    Reset {
        /// If groups are specified, only those specific groups will be reset.
        #[arg(short, long)]
        groups: Vec<String>,

        /// Don't ask for any confirmation.
        #[arg(short, long)]
        force: bool,

        /// Return before the reset has finished.
        #[arg(long)]
        no_wait: bool,
    },

    /// Remotely shut down the daemon. Should only be used if the daemon isn't started by a service
    /// manager.
    Shutdown,

    /// Set the amount of allowed parallel tasks
    ///
    /// By default, adjusts the amount of the default group.
    ///
    /// No tasks will be stopped, if this is lowered.
    /// This limit is only considered when tasks are scheduled.
    Parallel {
        /// The amount of allowed parallel tasks.
        ///
        /// Setting this to 0 means an unlimited amount of parallel tasks.
        parallel_tasks: Option<usize>,

        /// Set the amount for a specific group.
        #[arg(name = "group", short, long)]
        group: Option<String>,
    },

    /// Display client/daemon versions and supported automation features as JSON.
    Capabilities,

    /// Generates shell completion files.
    ///
    /// This can be ignored during normal operations.
    Completions {
        /// The target shell.
        #[arg(value_enum)]
        shell: Shell,
        /// The output directory to which the file should be written.
        #[arg(value_hint = ValueHint::DirPath)]
        output_directory: Option<PathBuf>,
    },
}

#[derive(Parser, Debug, Clone)]
pub enum EnvCommand {
    /// Set a variable for a specific task's environment.
    Set {
        /// The id of the task for which the variable should be set.
        task_id: usize,

        /// The name of the environment variable to set.
        key: String,

        /// The value of the environment variable to set.
        value: String,
    },

    /// Remove a specific variable from a task's environment.
    Unset {
        /// The id of the task for which the variable should be set.
        task_id: usize,

        /// The name of the environment variable to set.
        key: String,
    },
}

#[derive(Parser, Debug, Clone)]
pub enum GroupCommand {
    /// Add a group by name.
    Add {
        name: String,

        /// Set the amount of parallel tasks this group can have.
        ///
        /// Setting this to 0 means an unlimited amount of parallel tasks.
        #[arg(short, long)]
        parallel: Option<usize>,
    },

    /// Remove a group by name.
    /// This will move all tasks in this group to the default group!
    Remove { name: String },
}

#[derive(Parser, ValueEnum, Debug, Clone, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    Never,
    Always,
}

#[derive(Parser, ValueEnum, Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
    Nushell,
}

#[derive(Parser, Debug)]
#[command(
    name = "pueue",
    about = concat!(
        "Interact with the Pueue daemon\n\n",
        "Use the `--help` long form to get detailed help output on each subcommand!"
    ),
    author,
    version
)]
pub struct CliArguments {
    /// Verbose mode (-v, -vv, -vvv)
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Colorize the output; auto enables color output when connected to a tty.
    #[arg(long, value_enum, default_value = "auto")]
    pub color: ColorChoice,

    /// If provided, Pueue only uses this config file.
    ///
    /// This path can also be set via the "PUEUE_CONFIG_PATH" environment variable.
    /// The commandline option overwrites the environment variable!
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    pub config: Option<PathBuf>,

    /// The name of the profile that should be loaded from your config file.
    #[arg(short, long)]
    pub profile: Option<String>,

    #[command(subcommand)]
    pub cmd: Option<SubCommand>,
}

/// Reject option-like arguments that `add` would otherwise absorb into the command.
///
/// Clap's trailing positional arguments intentionally accept all remaining values. That is useful
/// for shell commands, but silently turns a misplaced pueue option into part of the command. An
/// explicit `--` remains available for commands whose own arguments start with a hyphen.
pub fn validate_cli_args<I, T>(args: I) -> std::result::Result<(), clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let mut index = 1;

    // Locate the subcommand while accounting for global options that take a separate value.
    while let Some(arg) = args.get(index) {
        let arg = arg.to_string_lossy();
        match arg.as_ref() {
            "--config" | "-c" | "--profile" | "-p" | "--color" => index += 2,
            "add" => return validate_add_args(&args[index + 1..]),
            _ if global_short_option_consumes_next(&arg) => index += 2,
            _ if arg.starts_with('-') => index += 1,
            _ => return Ok(()),
        }
    }

    Ok(())
}

fn global_short_option_consumes_next(arg: &str) -> bool {
    if !arg.starts_with('-') || arg.starts_with("--") {
        return false;
    }

    for (index, option) in arg.char_indices().skip(1) {
        match option {
            'v' => continue,
            'c' | 'p' => return index + option.len_utf8() == arg.len(),
            _ => return false,
        }
    }

    false
}

fn validate_add_args(args: &[OsString]) -> std::result::Result<(), clap::Error> {
    let mut command_started = false;
    let mut expects_value = false;

    for arg in args {
        let arg = arg.to_string_lossy();
        if expects_value {
            expects_value = false;
            continue;
        }

        if arg == "--" {
            break;
        }

        if !command_started {
            if matches!(
                arg.as_ref(),
                "--working-directory"
                    | "-w"
                    | "--delay"
                    | "-d"
                    | "--group"
                    | "-g"
                    | "--after"
                    | "-a"
                    | "--priority"
                    | "-o"
                    | "--label"
                    | "-l"
                    | "--env"
            ) {
                expects_value = true;
                continue;
            }

            if !arg.starts_with('-') {
                command_started = true;
            }
            continue;
        }

        if arg.starts_with('-') {
            return Err(CliArguments::command().error(
                clap::error::ErrorKind::UnknownArgument,
                format!(
                    "option `{arg}` appears after the command. \
                     Put pueue options before the command, or use \
                     `pueue add [OPTIONS] -- <COMMAND>...` for command arguments."
                ),
            ));
        }
    }

    Ok(())
}

fn parse_delay_until(src: &str) -> Result<DateTime<Local>, String> {
    if let Ok(seconds) = src.parse::<i64>() {
        let delay_until = Local::now()
            + TimeDelta::try_seconds(seconds)
                .ok_or(format!("Failed to get timedelta from {seconds} seconds"))?;
        return Ok(delay_until);
    }

    if let Ok(date_time) = parse_date_string(src, Local::now(), Dialect::Us) {
        return Ok(date_time);
    }

    Err(String::from(
        "could not parse as seconds or date expression",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_rejects_options_after_the_command() {
        let error = validate_cli_args(["pueue", "add", "echo", "hello", "--stashed"])
            .expect_err("a misplaced pueue option must be rejected");

        assert!(error.to_string().contains("appears after the command"));
    }

    #[test]
    fn add_validation_handles_clustered_global_options() {
        let cases = [
            vec!["pueue", "-vc", "/tmp/config", "add", "echo", "--stashed"],
            vec!["pueue", "-vp", "dev", "add", "echo", "--stashed"],
            vec!["pueue", "-vc/tmp/config", "add", "echo", "--stashed"],
            vec!["pueue", "-vpdev", "add", "echo", "--stashed"],
        ];

        for args in cases {
            validate_cli_args(args).expect_err("misplaced options must be rejected");
        }
    }

    #[test]
    fn add_accepts_command_options_after_a_separator() {
        let args = [
            "pueue",
            "add",
            "--after=1,2",
            "--after",
            "3",
            "--stashed",
            "--",
            "echo",
            "-n",
        ];
        validate_cli_args(args).unwrap();
        let parsed = CliArguments::try_parse_from(args).unwrap();

        let Some(SubCommand::Add {
            command,
            dependencies,
            stashed,
            ..
        }) = parsed.cmd
        else {
            panic!("expected add subcommand");
        };
        assert_eq!(command, ["echo", "-n"]);
        assert_eq!(dependencies, [1, 2, 3]);
        assert!(stashed);
    }

    #[test]
    fn add_keeps_the_existing_unseparated_form() {
        validate_cli_args(["pueue", "add", "echo", "hello world"]).unwrap();
    }
}
