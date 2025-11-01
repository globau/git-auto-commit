use std::fs;
use std::path::Path;

#[test]
fn no_todo_comments() {
    let mut todos = Vec::new();

    // search all rust source files for todo comments
    let src_dir = Path::new("src");
    if src_dir.exists() {
        search_dir(src_dir, &mut todos);
    }

    if !todos.is_empty() {
        eprintln!("\nfound {} TODO comment(s):", todos.len());
        for (file, line_num, line) in &todos {
            eprintln!("  {}:{}: {}", file, line_num, line.trim());
        }
        panic!("todo comments must be removed before tests pass");
    }
}

fn search_dir(dir: &Path, todos: &mut Vec<(String, usize, String)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                search_dir(&path, todos);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                search_file(&path, todos);
            }
        }
    }
}

fn search_file(path: &Path, todos: &mut Vec<(String, usize, String)>) {
    if let Ok(content) = fs::read_to_string(path) {
        for (line_num, line) in content.lines().enumerate() {
            // check if this line contains a todo in a comment
            if is_todo_in_comment(line) {
                todos.push((path.display().to_string(), line_num + 1, line.to_string()));
            }
        }
    }
}

fn is_todo_in_comment(line: &str) -> bool {
    let line_upper = line.to_uppercase();

    // check for line comments: // TODO
    if let Some(comment_pos) = line.find("//") {
        let comment_part = &line_upper[comment_pos..];
        if comment_part.contains("TODO") {
            return true;
        }
    }

    // check for block comments: /* TODO */ or continuation lines starting with *
    if let Some(block_start) = line.find("/*") {
        let comment_part = &line_upper[block_start..];
        if comment_part.contains("TODO") {
            return true;
        }
    }

    // check for block comment continuation lines (e.g., " * TODO")
    let trimmed = line.trim_start();
    if trimmed.starts_with('*') && !trimmed.starts_with("*/") && line_upper.contains("TODO") {
        return true;
    }

    false
}
