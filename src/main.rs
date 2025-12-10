mod claude;
mod cli;
mod constants;
mod context;
mod git;
mod ui;

use crate::constants::{
    DEFAULT_CONTEXT, DIFF_SIZE_MAXIMUM_BYTES, DIFF_SIZE_WARNING_BYTES, LESS_CONTEXT,
    MAX_AUTO_REROLLS, MAX_FILES_TO_SHOW, MAX_LINE_LENGTH, MODEL_FAST, MODEL_SMART,
};
use crate::git::{ChangeSet, FileChange, status_char};
use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use num_format::{Locale, ToFormattedString};
use std::io::IsTerminal;
use std::path::Path;

fn main() {
    if let Err(e) = run() {
        error!("{}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // parse cli arguments
    let cli = cli::Cli::parse_args();

    // sanity checks
    if !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal()
        || !std::io::stderr().is_terminal()
    {
        bail!("interactive terminal required");
    }
    git::sanity_check()?;

    // create application context
    let mut ctx = context::AppContext::new(cli.debug_prompt, cli.debug_response);

    // main - try with default context first, reduce if necessary
    let changeset = loop {
        match git::get_changes(Path::new("."), ctx.context_lines)? {
            Some(cs) => {
                let diff_size = cs.diff.len();

                if diff_size <= DIFF_SIZE_WARNING_BYTES {
                    // diff is acceptable size, use it
                    break cs;
                }
                if diff_size > DIFF_SIZE_WARNING_BYTES && ctx.context_lines == DEFAULT_CONTEXT {
                    // diff is too large, try with less context
                    ctx.context_lines = LESS_CONTEXT;
                    continue;
                }

                let diff_size_str = diff_size.to_formatted_string(&Locale::en);

                // already tried with less context, check maximum and warn
                if diff_size > DIFF_SIZE_MAXIMUM_BYTES {
                    bail!(
                        "diff is too large ({diff_size_str} chars, max {}k)",
                        DIFF_SIZE_MAXIMUM_BYTES / 1024
                    );
                }
                warning!("diff is large ({diff_size_str} chars), this may use many tokens");
                let response = ui::prompt(&["continue", "abort"])?;
                if response == "a" {
                    std::process::exit(1);
                }
                break cs;
            }
            None => bail!("no changes found"),
        }
    };

    process_changes(&mut ctx, &changeset)?;
    Ok(())
}

fn process_changes(ctx: &mut context::AppContext, changeset: &ChangeSet) -> Result<()> {
    loop {
        // switch to a smarter model when rerolling
        if ctx.manual_reroll_count > 0 || ctx.auto_reroll_count > 0 {
            ctx.model = MODEL_SMART.to_string();
        }

        // regenerate commit desc, if required
        if ctx.regenerate
            && let Some(desc) = generate(ctx, changeset)
        {
            if desc.trim().is_empty() {
                warning!("generated description is empty, using fallback");
            } else {
                ctx.commit_description = desc;
                ctx.user_edited = false;
            }
        }
        ctx.model = MODEL_FAST.to_string();
        ctx.regenerate = true;
        ctx.think_hard = false;

        // display commit info
        display_commit_info(&ctx.commit_description, &changeset.files);

        // auto-reroll long lines (claude frequently ignores the 72 char limit)
        // but only if the description was not user-edited
        if !ctx.user_edited {
            let any_line_too_long = ctx
                .commit_description
                .lines()
                .any(|line| line.len() > MAX_LINE_LENGTH);
            if any_line_too_long {
                let message = format!(
                    "commit message {} longer than {} chars",
                    if ctx.commit_description.lines().count() > 1 {
                        "has lines"
                    } else {
                        "is"
                    },
                    MAX_LINE_LENGTH
                );
                if ctx.auto_reroll_count >= MAX_AUTO_REROLLS {
                    error!(
                        "{} (not auto-rerolling after {} attempts)",
                        message, MAX_AUTO_REROLLS
                    );
                } else {
                    error!("{}, rerolling...", message);
                    ctx.auto_reroll_count += 1;
                    ctx.think_hard = true;
                    continue;
                }
            }
            ctx.auto_reroll_count = 0;
        }

        // display warnings
        if ctx.commit_description.to_lowercase().contains("claude") {
            warning!("warning: commit desc contains a reference to Claude");
        }
        if !ctx.multi_line && ctx.commit_description.contains('\n') {
            warning!("warning: commit message contains multiple lines");
        }

        // prompt user and handle action
        let options = [
            "YES",
            "no",
            "reroll",
            if ctx.multi_line { "short" } else { "long" },
            "edit",
            "prompt",
        ];
        let action = ui::prompt(&options)?;
        match handle_user_action(&action, ctx)? {
            UserAction::Commit => break,
            UserAction::Exit => std::process::exit(1),
            UserAction::Reroll => {
                ctx.think_hard = true;
                ctx.manual_reroll_count += 1;
            }
            UserAction::Continue => {
                ctx.regenerate = false;
                ctx.manual_reroll_count = 0;
            }
        }
    }

    // commit
    if !changeset.is_staged {
        git::stage(Path::new("."), changeset)?;
    }
    git::commit(Path::new("."), &ctx.commit_description)?;

    Ok(())
}

enum UserAction {
    Commit,
    Exit,
    Reroll,
    Continue,
}

/// generate commit description with spinner
fn generate(ctx: &context::AppContext, changeset: &ChangeSet) -> Option<String> {
    let file_count = changeset.files.len();
    let summary = format!(
        "{} [{} {}]",
        changeset.source(),
        file_count,
        if file_count == 1 { "file" } else { "files" }
    );

    let spinner = if ctx.debug_prompt {
        None
    } else {
        let s = ProgressBar::new_spinner();
        s.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg:.cyan}")
                .unwrap(),
        );
        s.set_message(format!("generating commit description from {summary}"));
        s.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(s)
    };

    let generated = claude::generate(ctx, changeset);

    if let Some(s) = spinner {
        s.finish_and_clear();
    }

    let generated = match generated {
        Ok(res) => res,
        Err(e) => {
            error!("{}", e);
            return None;
        }
    };

    status!(
        "{} ({} tokens, ${:.4} USD)",
        summary,
        generated.tokens.to_formatted_string(&Locale::en),
        generated.cost
    );

    Some(generated.message)
}

/// display commit description and files
fn display_commit_info(commit_description: &str, files: &[FileChange]) {
    use colored::Colorize;
    use std::io::{self, Write};

    /// print text with "claude" (case insensitive) highlighted in yellow
    fn print_with_claude_highlighted(text: &str) {
        let lower = text.to_lowercase();
        let mut last_end = 0;

        while let Some(pos) = lower[last_end..].find("claude") {
            let absolute_pos = last_end + pos;

            // print the part before "claude"
            let before = &text[last_end..absolute_pos];
            if !before.is_empty() {
                let _ = write!(io::stdout(), "{before}");
            }

            // print "claude" in yellow
            let claude_end = absolute_pos + "claude".len();
            let claude_part = &text[absolute_pos..claude_end];
            let _ = write!(io::stdout(), "{}", claude_part.yellow());

            last_end = claude_end;
        }

        // print the remaining part
        if last_end < text.len() {
            let remaining = &text[last_end..];
            let _ = write!(io::stdout(), "{remaining}");
        }
    }

    // print each line of commit description, highlighting chars beyond MAX_LINE_LENGTH-1 in red
    // and highlighting "claude" in yellow (red overrides yellow for long lines)
    let _ = writeln!(io::stdout());
    for line in commit_description.lines() {
        if line.len() <= MAX_LINE_LENGTH {
            print_with_claude_highlighted(line);
            let _ = writeln!(io::stdout());
        } else {
            let (first_part, rest) = line.split_at(MAX_LINE_LENGTH);
            print_with_claude_highlighted(first_part);
            let _ = write!(io::stdout(), "{}", rest.red());
            let _ = writeln!(io::stdout());
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
fn handle_user_action(action: &str, ctx: &mut context::AppContext) -> Result<UserAction> {
    match action {
        "y" => Ok(UserAction::Commit),
        "n" => Ok(UserAction::Exit),
        "r" => Ok(UserAction::Reroll),
        "s" => {
            ctx.multi_line = false;
            ctx.commit_description = ctx
                .commit_description
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            Ok(UserAction::Continue)
        }
        "l" => {
            ctx.multi_line = true;
            Ok(UserAction::Reroll)
        }
        "e" => {
            ctx.commit_description = if ctx.multi_line {
                ui::edit_multi_line(&ctx.commit_description)?
            } else {
                info!("");
                ui::edit_one_line(&ctx.commit_description)?
            };
            if ctx.commit_description.trim().is_empty() {
                std::process::exit(1);
            }
            ctx.user_edited = true;
            Ok(UserAction::Continue)
        }
        "p" => {
            status!("provide extra claude prompt context:");
            let old_prompt_extra = ctx.prompt_extra.clone();
            ctx.prompt_extra = ui::edit_one_line(&ctx.prompt_extra)?;
            if ctx.prompt_extra == old_prompt_extra {
                Ok(UserAction::Continue)
            } else {
                Ok(UserAction::Reroll)
            }
        }
        _ => Ok(UserAction::Continue),
    }
}
