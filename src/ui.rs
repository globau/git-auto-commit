#[macro_export]
macro_rules! warning {
    ($msg:expr) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stderr(), "{}", $msg.yellow());
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let msg = format!($fmt, $($arg)*);
        let _ = writeln!(io::stderr(), "{}", msg.yellow());
    }};
}

#[macro_export]
macro_rules! error {
    ($msg:expr) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stderr(), "{}", $msg.red());
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let msg = format!($fmt, $($arg)*);
        let _ = writeln!(io::stderr(), "{}", msg.red());
    }};
}

#[macro_export]
macro_rules! fatal {
      // fatal!("aiee"; 3)
      ($msg:expr; $code:expr) => {{
          use colored::Colorize;
          use std::io::{self, Write};
          let _ = writeln!(io::stderr(), "{}", $msg.red());
          std::process::exit($code);
      }};

      // fatal!("oh no: {}", "aiee", 3)
      ($fmt:expr, $($arg:expr),+; $code:expr) => {{
          use colored::Colorize;
          use std::io::{self, Write};
          let msg = format!($fmt, $($arg),+);
          let _ = writeln!(io::stderr(), "{}", msg.red());
          std::process::exit($code);
      }};

      // fatal!("aiee")
      ($msg:expr) => {{
          use colored::Colorize;
          use std::io::{self, Write};
          let _ = writeln!(io::stderr(), "{}", $msg.red());
          std::process::exit(1);
      }};

      // fatal!("oh no: {}", "aiee")
      ($fmt:expr, $($arg:expr),+) => {{
          use colored::Colorize;
          use std::io::{self, Write};
          let msg = format!($fmt, $($arg),+);
          let _ = writeln!(io::stderr(), "{}", msg.red());
          std::process::exit(1);
      }};
  }

#[macro_export]
macro_rules! title {
    ($msg:expr) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), "{}", $msg.green());
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        use colored::Colorize;
        use std::io::{self, Write};
        let msg = format!($fmt, $($arg)*);
        let _ = writeln!(io::stdout(), "{}", msg.green());
    }};
}

#[macro_export]
macro_rules! output {
    () => {{
        use std::io::{self, Write};
        let _ = writeln!(io::stdout());
    }};
    ($msg:expr) => {{
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), "{}", $msg);
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        use std::io::{self, Write};
        let _ = writeln!(io::stdout(), $fmt, $($arg)*);
    }};
}

pub fn prompt(options: &[&str]) -> String {
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
            let first = opt.chars().next().unwrap();
            let rest = &opt[first.len_utf8()..];
            format!("[{first}]{rest}")
        })
        .collect();

    // build valid characters (first char of each option, lowercased)
    let valid_chars: Vec<char> = options
        .iter()
        .map(|opt| opt.chars().next().unwrap().to_lowercase().next().unwrap())
        .collect();

    // print the prompt
    print!("{} ? ", prompt_parts.join("/"));
    let _ = io::stdout().flush();

    // enable raw mode for single-character input
    enable_raw_mode().unwrap_or_else(|_| {
        fatal!("this command requires an interactive terminal");
    });

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
                    output!("^C");
                    std::process::exit(3);
                }
                // handle ctrl-c
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    disable_raw_mode().ok();
                    output!("^C");
                    std::process::exit(3);
                } // handle enter (use first option as default)
                KeyCode::Enter => {
                    let ch = valid_chars[0];
                    disable_raw_mode().ok();
                    output!(ch);
                    break ch.to_string();
                }
                // handle valid character input
                KeyCode::Char(c) => {
                    let lower = c.to_lowercase().next().unwrap();
                    if valid_chars.contains(&lower) {
                        disable_raw_mode().ok();
                        output!(lower);
                        break lower.to_string();
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn edit_one_line(line: &str) -> String {
    use rustyline::DefaultEditor;

    // create a rustyline editor
    let mut editor = DefaultEditor::new().unwrap_or_else(|_| {
        fatal!("failed to initialise line editor");
    });

    // show the prompt and pre-filled text
    if let Ok(edited) = editor.readline_with_initial("? ", (line, "")) {
        edited
    } else {
        output!("^C");
        std::process::exit(3);
    }
}

pub fn edit_multi_line(text: &str) -> String {
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::process::Command;
    use tempfile::Builder;

    // get the EDITOR environment variable
    let editor = env::var("EDITOR").unwrap_or_else(|_| {
        fatal!("EDITOR not set");
    });

    // create a temporary file with .tmp suffix
    let mut temp_file = Builder::new()
        .suffix(".tmp")
        .tempfile()
        .unwrap_or_else(|_| {
            fatal!("failed to create temporary file");
        });

    // write the initial text to the file
    temp_file.write_all(text.as_bytes()).unwrap_or_else(|_| {
        fatal!("failed to write to temporary file");
    });

    // get the path before the file is closed
    let temp_path = temp_file.path().to_owned();

    // flush to ensure content is written
    temp_file.flush().unwrap_or_else(|_| {
        fatal!("failed to flush temporary file");
    });

    // run the editor via shell to properly handle arguments in EDITOR
    let editor_command = format!(
        "{} {}",
        editor,
        shlex::try_quote(&temp_path.to_string_lossy()).unwrap()
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&editor_command)
        .status()
        .unwrap_or_else(|_| {
            fatal!("failed to run editor: {}", editor);
        });

    if !status.success() {
        fatal!("cancelled"; 3);
    }

    // read back the edited content
    let edited = fs::read_to_string(&temp_path)
        .unwrap_or_else(|_| String::new())
        .trim()
        .to_string();

    if edited.is_empty() {
        fatal!("cancelled"; 3);
    }

    // temp_file will be automatically cleaned up when it goes out of scope
    edited
}
