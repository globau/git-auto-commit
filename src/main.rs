mod claude;
mod git;
mod ui;

use crate::git::{ChangeSet, FileChange, status_char};
use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;
use std::path::Path;

const MAX_LINE_LENGTH: usize = 72;
const MAX_FILES_TO_SHOW: usize = 10;
const MAX_AUTO_REROLLS: usize = 3;

fn main() {
    if let Err(e) = run() {
        error!("{}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // sanity checks
    if !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal()
        || !std::io::stderr().is_terminal()
    {
        bail!("interactive terminal required");
    }
    git::sanity_check()?;

    // main
    match git::get_changes(Path::new("."))? {
        Some(changeset) => process_changes(&changeset)?,
        None => bail!("no changes found"),
    }

    Ok(())
}

fn process_changes(changeset: &ChangeSet) -> Result<()> {
    let file_count = changeset.files.len();
    let file_word = if file_count == 1 { "file" } else { "files" };

    status!(
        "generating commit description from {} touching {} {}...",
        changeset.source(),
        file_count,
        file_word
    );

    // check diff size and enforce limits
    let diff_size = changeset.diff.len();
    if diff_size > 100 * 1024 {
        bail!("diff is too large ({diff_size} chars, max 100k)");
    } else if diff_size > 50 * 1024 {
        warning!("diff is large ({diff_size} chars), this may use many tokens");
        let response = ui::prompt(&["continue", "abort"])?;
        if response == "a" {
            std::process::exit(1);
        }
    }

    let mut multi_line = false;
    let mut think_hard = false;
    let mut regenerate = true;
    let mut prompt_extra = String::new();
    let mut commit_description = String::from("bug fixes and/or improvements");
    let mut auto_reroll_count = 0;

    loop {
        // regenerate commit desc, if required
        if regenerate && let Some(desc) = generate(changeset, multi_line, think_hard, &prompt_extra)
        {
            if desc.trim().is_empty() {
                warning!("generated description is empty, using fallback");
            } else {
                commit_description = desc;
            }
        }
        regenerate = true;
        think_hard = false;

        // display commit info
        display_commit_info(&commit_description, &changeset.files);

        // auto-reroll long lines (claude frequently ignores the 72 char limit)
        let any_line_too_long = commit_description
            .lines()
            .any(|line| line.len() > MAX_LINE_LENGTH);
        if any_line_too_long {
            let message = format!(
                "commit message {} longer than {} chars",
                if commit_description.lines().count() > 1 {
                    "has lines"
                } else {
                    "is"
                },
                MAX_LINE_LENGTH
            );
            if auto_reroll_count >= MAX_AUTO_REROLLS {
                error!(
                    "{} (not auto-rerolling after {} attempts)",
                    message, MAX_AUTO_REROLLS
                );
            } else {
                error!("{}, rerolling...", message);
                auto_reroll_count += 1;
                think_hard = true;
                continue;
            }
        }
        auto_reroll_count = 0;

        // display warnings
        if commit_description.to_lowercase().contains("claude") {
            warning!("warning: commit desc contains a reference to Claude");
        }
        if !multi_line && commit_description.contains('\n') {
            warning!("warning: commit message contains multiple lines");
        }

        // prompt user and handle action
        let options = [
            "YES",
            "no",
            "reroll",
            if multi_line { "short" } else { "long" },
            "edit",
            "prompt",
        ];
        let action = ui::prompt(&options)?;
        match handle_user_action(
            &action,
            &mut commit_description,
            &mut multi_line,
            &mut prompt_extra,
        )? {
            UserAction::Commit => break,
            UserAction::Exit => std::process::exit(1),
            UserAction::Reroll => {
                think_hard = true;
            }
            UserAction::Continue => {
                regenerate = false;
            }
        }
    }

    // commit
    if !changeset.is_staged {
        git::stage(Path::new("."), changeset)?;
    }
    git::commit(Path::new("."), &commit_description)?;

    Ok(())
}

enum UserAction {
    Commit,
    Exit,
    Reroll,
    Continue,
}

/// generate commit description with spinner
fn generate(
    changeset: &ChangeSet,
    multi_line: bool,
    think_hard: bool,
    prompt_extra: &str,
) -> Option<String> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner}")
            .expect("invalid spinner template"),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let result = claude::generate(changeset, multi_line, think_hard, prompt_extra);

    spinner.finish_and_clear();

    match result {
        Ok(description) => Some(description),
        Err(e) => {
            error!("{}", e);
            None
        }
    }
}

/// display commit description and files
fn display_commit_info(commit_description: &str, files: &[FileChange]) {
    use colored::Colorize;
    use std::io::{self, Write};

    // print each line of commit description, highlighting chars beyond MAX_LINE_LENGTH-1 in red
    let _ = writeln!(io::stdout());
    for line in commit_description.lines() {
        if line.len() <= MAX_LINE_LENGTH {
            let _ = writeln!(io::stdout(), "{line}");
        } else {
            let (first_part, rest) = line.split_at(MAX_LINE_LENGTH);
            let _ = write!(io::stdout(), "{first_part}");
            let _ = writeln!(io::stdout(), "{}", rest.red());
        }
    }
    let _ = writeln!(io::stdout());

    status!("files:");

    let files_to_show = files.iter().take(MAX_FILES_TO_SHOW);

    for file in files_to_show {
        if let Some(old_path) = &file.old_path {
            // show renames as "old_path → new_path"
            info!("{} {} → {}", status_char(file.status), old_path, file.path);
        } else {
            info!("{} {}", status_char(file.status), file.path);
        }
    }

    // show count of remaining files if there are more than MAX_FILES_TO_SHOW
    if files.len() > MAX_FILES_TO_SHOW {
        let remaining = files.len() - MAX_FILES_TO_SHOW;
        info!("(+{} more)", remaining);
    }

    info!();
}

/// handle user action and return what to do next
fn handle_user_action(
    action: &str,
    commit_description: &mut String,
    multi_line: &mut bool,
    prompt_extra: &mut String,
) -> Result<UserAction> {
    match action {
        "y" => Ok(UserAction::Commit),
        "n" => Ok(UserAction::Exit),
        "r" => {
            status!("rerolling...");
            Ok(UserAction::Reroll)
        }
        "s" => {
            *multi_line = false;
            *commit_description = commit_description.lines().next().unwrap_or("").to_string();
            status!("updating...");
            Ok(UserAction::Continue)
        }
        "l" => {
            *multi_line = true;
            status!("thinking...");
            Ok(UserAction::Reroll)
        }
        "e" => {
            *commit_description = if *multi_line {
                ui::edit_multi_line(commit_description)?
            } else {
                info!("");
                ui::edit_one_line(commit_description)?
            };
            if commit_description.trim().is_empty() {
                std::process::exit(3);
            }
            status!("updating...");
            Ok(UserAction::Continue)
        }
        "p" => {
            status!("provide extra claude prompt context:");
            for line in claude::get_prompt(*multi_line).lines() {
                info!("> {}", line);
            }
            let old_prompt_extra = prompt_extra.clone();
            *prompt_extra = ui::edit_one_line(prompt_extra.as_str())?;
            status!("thinking...");
            if *prompt_extra == old_prompt_extra {
                Ok(UserAction::Continue)
            } else {
                Ok(UserAction::Reroll)
            }
        }
        _ => Ok(UserAction::Continue),
    }
}
