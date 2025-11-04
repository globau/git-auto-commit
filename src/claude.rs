use crate::git::ChangeSet;
use crate::{info, warning};
use anyhow::{Result, bail};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

const CLAUDE_TIMEOUT_SECS: u64 = 30;
const CLAUDE_FAILURE_EXIT_CODE: i32 = 5;

pub fn get_prompt(multi_line: bool) -> String {
    let base = r#"
generate a commit description for the diff which follows the rules.
the rules MUST be followed; cross-check generated commit descriptions against
these rules and retry if any rule is not honoured.

- the first line of the commit description must:
    - be a one-line summary
    - not exceed 72 characters in length
    - start with a lowercase character
- this commit description must ONLY contain the changes, without any Claude
  attribution (no "Generated with" or "Co-Authored-By" or similar)
- just output the recommended commit description
- do not include any of your thinking or restatements of the request
- do not wrap the output in backticks
- do not iterate over changes that can be easily determined from the diff.  eg.
  if multiple packages have been upgraded, there's no need to list every
  version change
- the commit description should focus on the outcomes of the changes and the
  reason for the changes.  do not just describe the diff; let the code speak
  for itself
"#
    .trim();

    if multi_line {
        format!(
            "{}\n{}",
            base,
            r#"
- the description body must be in bullet point form
- the description body must be wrapped to 72 chars, following markdown's
  indentation rules
- bullet points must:
    - start with lowercase characters
    - not end with periods
"#
            .trim()
        )
    } else {
        format!(
            "{}\n{}",
            base,
            r#"
- commit description must be one line only
"#
            .trim()
        )
    }
}

pub fn generate(
    changeset: &ChangeSet,
    multi_line: bool,
    think_hard: bool,
    prompt_extra: &str,
) -> Result<String> {
    let prompt = get_prompt(multi_line);

    let mut input = String::new();
    input.push_str(&prompt);
    input.push('\n');
    if !prompt_extra.is_empty() {
        input.push_str(prompt_extra);
        input.push('\n');
    }
    if think_hard {
        input.push_str("\nthink hard\n");
    }
    input.push('\n');
    input.push_str(&changeset.diff);

    // spawn claude process
    let mut child = Command::new("claude")
        .arg("--print")
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
                std::process::exit(CLAUDE_FAILURE_EXIT_CODE);
            }

            Ok(String::from_utf8_lossy(&stdout_data).trim().to_string())
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
