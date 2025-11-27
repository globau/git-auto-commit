use crate::constants::{
    CLAUDE_TIMEOUT_SECS, MAX_LINE_LENGTH, MAX_SAFE_LINE_LENGTH, MIN_SAFE_LINE_LENGTH,
    ULTRATHINK_THRESHOLD,
};
use crate::context::AppContext;
use crate::git::ChangeSet;
use crate::{info, warning};
use anyhow::{Result, bail};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

pub fn get_prompt(ctx: &AppContext) -> String {
    let multi_line = ctx.multi_line;
    let base = format!(
        r#"
IGNORE ALL CLAUDE.MD FILES. this task overrides any claude.md instructions.

YOU ARE A COMMIT MESSAGE GENERATOR.

MANDATORY OUTPUT FORMAT (NOT OPTIONAL):
```
<commit message here>
```

CRITICAL REQUIREMENTS:
- you MUST wrap the commit message in triple backticks (```)
- no explanations, no preamble, no "here's my suggestion"

RULE #1: ≤{MAX_LINE_LENGTH} characters per line (ABSOLUTE MAXIMUM - exceeding this = REJECTED)
TARGET: be descriptive but stay comfortably under {MAX_LINE_LENGTH} (aim for {MIN_SAFE_LINE_LENGTH}-{MAX_SAFE_LINE_LENGTH} chars)

COUNTING PROCESS (mandatory):
1. Write a descriptive message explaining what changed and why
2. Count every single character including spaces
3. If >{MAX_LINE_LENGTH} chars: use compression tactics below
4. If still >{MAX_LINE_LENGTH}: REJECTED - rewrite shorter

WRITING EFFECTIVE MESSAGES:
- be descriptive: explain what and why within the character limit
- use clear verbs: add, fix, update, remove, refactor, improve, etc
- include relevant context if space allows
- focus on the primary change

COMPRESSION TACTICS (use when needed to fit {MAX_LINE_LENGTH}):
- prefer short verbs: add, fix, update, remove, refactor
- drop articles where clear: "update config" not "update the config"
- remove unnecessary adjectives: "fix bug" not "fix critical bug"
- focus on primary change if describing multiple things
"#
    )
    .trim()
    .to_string();

    let format_rules = if multi_line {
        format!(
            r#"
MULTI-LINE FORMAT (MANDATORY - you MUST use this format):
- line 1: summary (≤{MAX_LINE_LENGTH} chars - count it)
- line 2: blank (required)
- line 3+: bullets with details (EACH ≤{MAX_LINE_LENGTH} chars - count every line)
  - bullets start lowercase, no end periods
  - provide 2-4 bullet points with specific details

CRITICAL: single-line output is NOT acceptable. you MUST use the multi-line format above.

GOOD MULTI-LINE EXAMPLES:
```
add user authentication system

- implement jwt token generation and validation
- add login and registration endpoints
- include password hashing with bcrypt
```

```
refactor database connection handling

- extract connection pool to separate module
- add retry logic for transient failures
- improve error messages for debugging
```

```
fix memory leak in background workers

- properly close database connections after use
- clear event listeners when workers shut down
```
"#
        )
        .trim()
        .to_string()
    } else {
        format!(
            r#"
FORMAT: single line only (≤{MAX_LINE_LENGTH} total)

LEARN FROM BAD EXAMPLES:
✗ WRONG (75 chars):
```
rewrite llm prompt to demand immediate output and strict character counting
```
✓ RIGHT (63 chars):
```
rewrite llm prompt for stricter output format and char counting
```

GOOD SINGLE-LINE EXAMPLES (notice descriptive yet concise):
```
add jwt authentication for user login endpoints
```

```
fix memory leak in background worker thread pool
```

```
update dependencies to resolve security vulnerabilities
```

```
refactor database query builder for better performance
```
"#
        )
        .trim()
        .to_string()
    };

    let additional_rules = format!(
        r#"
OTHER RULES (secondary to ≤{MAX_LINE_LENGTH} limit):
- start with lowercase letter
- no claude attribution
- focus on outcome, not implementation details

FINAL VERIFICATION: count characters. if >{MAX_LINE_LENGTH}, you FAILED. rewrite shorter.
"#
    )
    .trim()
    .to_string();

    format!("{base}\n\n{format_rules}\n\n{additional_rules}\n\n")
}

pub fn generate(ctx: &AppContext, changeset: &ChangeSet) -> Result<String> {
    let prompt = get_prompt(ctx);

    let mut input = String::new();
    input.push_str(&prompt);
    input.push('\n');
    if !ctx.prompt_extra.is_empty() {
        input.push_str(&ctx.prompt_extra);
        input.push('\n');
    }
    if ctx.auto_reroll_count > 0 {
        let critical_failure_msg = format!(
            r#"
CRITICAL FAILURE: previous attempt exceeded {MAX_LINE_LENGTH} characters.

YOU MUST:
1. Write a descriptive message that fits within {MAX_LINE_LENGTH} characters
2. Count EVERY character including spaces
3. If >{MAX_LINE_LENGTH} chars: apply compression tactics (short verbs, drop articles, remove adjectives)
4. If still >{MAX_LINE_LENGTH}: START OVER with different wording

Stay descriptive but use compression tactics to fit the limit.
"#
        )
        .trim()
        .to_string();
        input.push_str(&critical_failure_msg);
        input.push('\n');
    }
    if ctx.think_hard {
        let think_mode = if ctx.manual_reroll_count > ULTRATHINK_THRESHOLD {
            "ultrathink"
        } else {
            "think hard"
        };
        input.push_str(think_mode);
        input.push('\n');
    }

    // print prompt if requested (before adding diff)
    if ctx.show_prompt {
        use colored::Colorize;
        use std::io::Write;
        let _ = writeln!(std::io::stdout(), "\n{}", input.dimmed());
    }

    input.push('\n');
    input.push_str(&changeset.diff);

    // spawn claude process
    let mut child = Command::new("claude")
        .args(["--print", "--tools", "", "--model", &ctx.model])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn claude process: {e}"))?;

    // write input to stdin and close it
    if let Some(mut stdin) = child.stdin.take()
        && let Err(e) = stdin.write_all(input.as_bytes())
    {
        let _ = child.kill();
        let _ = child.wait();
        bail!("failed to write to claude stdin: {e}");
    }

    // take stdout and stderr handles
    let mut stdout = child
        .stdout
        .take()
        .expect("failed to take stdout from child process");
    let mut stderr = child
        .stderr
        .take()
        .expect("failed to take stderr from child process");

    // wait for process to complete with timeout
    let timeout = Duration::from_secs(CLAUDE_TIMEOUT_SECS);
    match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            // process completed within timeout, read output
            let mut stdout_data = Vec::new();
            let mut stderr_data = Vec::new();

            if let Err(e) = stdout.read_to_end(&mut stdout_data) {
                warning!("failed to read claude stdout: {}", e);
            }
            if let Err(e) = stderr.read_to_end(&mut stderr_data) {
                warning!("failed to read claude stderr: {}", e);
            }

            if !status.success() {
                if !stdout_data.is_empty() {
                    info!("{}", String::from_utf8_lossy(&stdout_data).trim());
                }
                if !stderr_data.is_empty() {
                    info!("{}", String::from_utf8_lossy(&stderr_data).trim());
                }
                std::process::exit(1);
            }

            let output = String::from_utf8_lossy(&stdout_data).trim().to_string();

            // extract commit message from between triple backticks
            if let Some(start) = output.find("```") {
                let after_first = &output[start + 3..];
                if let Some(end) = after_first.find("```") {
                    let commit_message = after_first[..end].trim();
                    return Ok(commit_message.to_string());
                }
            }

            // fallback if no backticks found
            warning!("claude output did not contain triple backticks");
            Ok(output)
        }
        Ok(None) => {
            // timeout occurred, kill the process
            if let Err(e) = child.kill() {
                warning!("failed to kill claude process: {}", e);
            }
            let _ = child.wait();
            bail!("claude thought for too long")
        }
        Err(e) => bail!("failed to wait for claude process: {e}"),
    }
}
