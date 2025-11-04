use anyhow::{Context, Result};

#[macro_export]
macro_rules! warning {
    // format string literal (with or without inline formatting)
    ($fmt:literal $(, $($arg:tt)*)?) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stderr(), "{}", format!($fmt $(, $($arg)*)?).yellow());
    }};
    // arbitrary expression (non-literal)
    ($expr:expr) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stderr(), "{}", format!("{}", $expr).yellow());
    }};
}

#[macro_export]
macro_rules! error {
    // format string literal (with or without inline formatting)
    ($fmt:literal $(, $($arg:tt)*)?) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stderr(), "{}", format!($fmt $(, $($arg)*)?).red());
    }};
    // arbitrary expression (non-literal)
    ($expr:expr) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stderr(), "{}", format!("{}", $expr).red());
    }};
}

#[macro_export]
macro_rules! status {
    // format string literal (with or without inline formatting)
    ($fmt:literal $(, $($arg:tt)*)?) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), "{}", format!($fmt $(, $($arg)*)?).green());
    }};
    // arbitrary expression (non-literal)
    ($expr:expr) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), "{}", format!("{}", $expr).green());
    }};
}

#[macro_export]
macro_rules! info {
    () => {{
        use std::io::{self, Write};
        let _ = writeln!(io::stdout());
    }};
    // format string literal (with or without inline formatting or args)
    ($fmt:literal $(, $($arg:tt)*)?) => {{
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), $fmt $(, $($arg)*)?);
    }};
    // arbitrary expression (non-literal)
    ($expr:expr) => {{
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), "{}", $expr);
    }};
}

pub fn prompt(options: &[&str]) -> Result<String> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        terminal::{disable_raw_mode, enable_raw_mode},
    };
    use std::io::{self, Write};

    // validate options are not empty (programming error if violated)
    debug_assert!(!options.is_empty(), "prompt requires at least one option");
    debug_assert!(
        options.iter().all(|opt| !opt.is_empty()),
        "prompt options cannot be empty strings"
    );

    // build prompt string like "[Y]ES/[n]o/[m]aybe"
    let prompt_parts: Vec<String> = options
        .iter()
        .map(|opt| {
            let first = opt
                .chars()
                .next()
                .expect("option should have at least one character");
            let rest = &opt[first.len_utf8()..];
            format!("[{first}]{rest}")
        })
        .collect();

    // build valid characters (first char of each option, lowercased)
    let valid_chars: Vec<char> = options
        .iter()
        .map(|opt| {
            opt.chars()
                .next()
                .expect("option should have at least one character")
                .to_lowercase()
                .next()
                .expect("lowercase should produce at least one character")
        })
        .collect();

    // print the prompt
    print!("{} ? ", prompt_parts.join("/"));
    let _ = io::stdout().flush();

    // enable raw mode for single-character input
    enable_raw_mode().context("this command requires an interactive terminal")?;

    loop {
        // read a key event
        if let Ok(Event::Key(KeyEvent {
            code, modifiers, ..
        })) = event::read()
        {
            match code {
                // handle esc
                KeyCode::Esc => {
                    disable_raw_mode().ok();
                    info!("^C");
                    std::process::exit(1);
                }
                // handle ctrl-c
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    disable_raw_mode().ok();
                    info!("^C");
                    std::process::exit(1);
                } // handle enter (use first option as default)
                KeyCode::Enter => {
                    let ch = valid_chars[0];
                    disable_raw_mode().ok();
                    info!(options[0]);
                    break Ok(ch.to_string());
                }
                // handle valid character input
                KeyCode::Char(c) => {
                    let lower = c
                        .to_lowercase()
                        .next()
                        .expect("lowercase should produce at least one character");
                    if let Some(idx) = valid_chars.iter().position(|&ch| ch == lower) {
                        disable_raw_mode().ok();
                        info!(options[idx]);
                        break Ok(lower.to_string());
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn edit_one_line(line: &str) -> Result<String> {
    use rustyline::DefaultEditor;

    // create a rustyline editor
    let mut editor = DefaultEditor::new().context("failed to initialise line editor")?;

    // show the prompt and pre-filled text
    if let Ok(edited) = editor.readline_with_initial("? ", (line, "")) {
        Ok(edited.trim().to_string())
    } else {
        info!("^C");
        std::process::exit(1);
    }
}

pub fn edit_multi_line(text: &str) -> Result<String> {
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::process::Command;
    use tempfile::Builder;

    // get the EDITOR environment variable
    let editor = env::var("EDITOR").context("EDITOR not set")?;

    // create a temporary file with .tmp suffix
    let mut temp_file = Builder::new()
        .suffix(".tmp")
        .tempfile()
        .context("failed to create temporary file")?;

    // write the initial text to the file
    temp_file
        .write_all(text.as_bytes())
        .context("failed to write to temporary file")?;

    // get the path before the file is closed
    let temp_path = temp_file.path().to_owned();

    // flush to ensure content is written
    temp_file
        .flush()
        .context("failed to flush temporary file")?;

    // run the editor via shell to properly handle arguments in EDITOR
    let editor_command = format!(
        "{} {}",
        editor,
        shlex::try_quote(&temp_path.to_string_lossy()).expect("path quoting should not fail")
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&editor_command)
        .status()
        .with_context(|| format!("failed to run editor: {editor}"))?;

    if !status.success() {
        std::process::exit(1);
    }

    // read back the edited content
    let edited = fs::read_to_string(&temp_path)
        .unwrap_or_else(|_| String::new())
        .trim()
        .to_string();

    if edited.is_empty() {
        std::process::exit(1);
    }

    // temp_file will be automatically cleaned up when it goes out of scope
    Ok(edited)
}
