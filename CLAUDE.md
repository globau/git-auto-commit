# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`git-auto-commit` is a Rust CLI tool that analyses git changes and displays files touched with their change types (A/M/D). The tool prioritises staged changes, falling back to unstaged changes (including untracked files) if nothing is staged.

## Build and Test

- Build: `cargo build --release` or `make build`
- Run: `cargo run` (uses debug build) or `./target/release/git-auto-commit`
- Run with debug: `./target/release/git-auto-commit --debug-prompt` (shows prompt sent to Claude) or `--debug-response` (shows full JSON response)
- Test suite: `make test` (runs formatting check, clippy with pedantic warnings, and tests)
- Format: `make format` or `cargo fmt`
- Install: `make install` (installs binary to Cargo's bin directory)

## Code Architecture

The codebase has several modules:

**`src/git.rs`**: Git operations module with hybrid implementation:
- Uses `git2` crate for reading diffs and staging files
- Uses git binary for creating commits (to support commit signing and git hooks)
- Handles all git states: additions (A), modifications (M), deletions (D), renames (R)
- Rename detection is enabled via `DiffFindOptions` - renames and moves show as single operations with status 'R'
- Automatically ignores diffs for: binary files, lock files (*.lock, *-lock.json/yaml), minified files (*.min.js/css)
- Contains data structures: `FileChange` (status, path, old_path, diff_ignored flag) and `ChangeSet` (collection of file changes with diff text)

**`src/ui.rs`**: User interface utilities including:
- Output macros: `warning!`, `error!`, `fatal!`, `title!`, `output!`
- `prompt()` function: displays interactive prompts with single-character input using the `crossterm` crate
  - Formats options as `[Y]ES/[n]o/[m]aybe ? ` (first char highlighted)
  - Uses raw terminal mode for immediate character input (no Enter key required)
  - Handles Esc/Ctrl-C to exit, Enter to select default (first option)
  - Requires an interactive terminal (TTY)
- `edit_one_line()` function: single-line editor using the `rustyline` crate
  - Takes an initial string and displays it for editing
  - Provides readline-style editing with cursor movement and editing capabilities
  - Returns the edited string
  - Handles Ctrl-C/Ctrl-D to exit with status code 3
- `edit_multi_line()` function: multi-line editor using the $EDITOR environment variable
  - Creates a temporary file with .tmp suffix containing initial text
  - Launches user's preferred editor from $EDITOR
  - Reads back the edited content after editor closes
  - Handles empty content or non-zero exit to exit with status code 3

**`src/cli.rs`**: Command-line argument parsing using `clap`
- Defines CLI structure with `--debug-prompt` flag for showing the prompt sent to Claude
- Defines CLI structure with `--debug-response` flag for showing the full JSON response from Claude
- Provides `parse_args()` method to parse command-line arguments

**`src/constants.rs`**: Application constants
- Commit message constraints: 72-char max line length, 60-70 char safe range
- UI settings: 10 files max display, 3 max auto-rerolls
- Diff settings: 3 default context lines, 1 reduced context lines, 50KB warning threshold, 100KB maximum size
- Claude settings: 30-second timeout, model names ("Haiku" fast, "Sonnet" smart), ultrathink threshold (2 manual rerolls)

**`src/claude.rs`**: LLM integration for commit message generation
- Supports two backends for Claude API access:
  - Direct API: uses `ureq` HTTP client to call Claude API directly (used when API key is configured)
  - CLI wrapper: spawns `claude` CLI tool as subprocess (used as fallback when no API key)
- API key configuration: reads from `~/.config/git-auto-commit/config` with format `api-key=<key>`
- Supports model selection: starts with fast model (Haiku), switches to smart model (Sonnet) after first reroll
- Model mapping: "Haiku" → claude-3-5-haiku-20241022, "Sonnet" → claude-3-7-sonnet-20250219
- Includes "think hard" mode for improved generation quality (enabled on rerolls)
- Includes "ultrathink" mode activated after 2+ consecutive manual rerolls for maximum quality
- Returns token usage and cost information (if available)
- Supports single-line and multi-line commit message formats
- Has configurable prompt with strict rules: 72-char limit, lowercase start, no Claude attribution
- 30-second timeout for LLM generation (configured via `ureq::Agent` for API, `wait_timeout` for CLI)
- Allows extra user-provided prompt context
- Functions take `&AppContext` to access all generation settings (model, multi_line, think_hard, prompt_extra, debug_prompt, debug_response, manual_reroll_count)

**`src/context.rs`**: Application state management
- `AppContext` struct holds all mutable state throughout the application
- Created in `run()` and passed through the call chain as `&mut AppContext`
- Bundles related state: git diff settings (context_lines), generation settings (model, multi_line, think_hard, prompt_extra), workflow flags (user_edited, regenerate, auto_reroll_count, manual_reroll_count), debugging flags (debug_prompt, debug_response), and the commit description itself
- Eliminates long parameter lists by passing a single context struct
- Uses `#[allow(clippy::struct_excessive_bools)]` as the bools represent independent flags rather than a state machine

**`src/main.rs`**: Main application workflow
1. Checks for staged changes first
2. Falls back to unstaged changes (including untracked files) if nothing staged
3. Automatic diff size management:
   - Starts with 3 context lines for git diff
   - If diff exceeds 50KB, reduces to 1 context line and retries
   - If diff still exceeds 100KB, exits with error
   - If diff is 50-100KB, prompts user to continue or abort
4. Generates commit description via Claude LLM using fast model (Haiku)
5. Displays generation summary with token usage and cost in USD
6. Interactive loop with options:
   - YES: accept and commit
   - no: abort
   - reroll: regenerate description (switches to smart model/Sonnet with "think hard" mode)
   - long/short: toggle between multi-line and single-line formats
   - edit: manually edit description (single-line or multi-line based on current format)
   - prompt: add extra context to Claude prompt
7. Model selection: starts with Haiku, switches to Sonnet after first reroll (auto or manual)
8. Ultrathink mode: activates after 2+ consecutive manual rerolls for enhanced quality
9. Auto-rerolls (max 3 attempts) if any line exceeds 72 characters
10. Warns if commit message contains "Claude" or has unexpected newlines
11. Displays up to 10 files in output (with count of remaining files if more)
12. Stages unstaged changes before committing (if applicable)

## Test Suite

The project includes comprehensive tests in `src/git/tests.rs` that verify:
- File renames (detected as single 'R' operation with old_path and new path)
- File moves to subdirectories (detected as single 'R' operation)
- Mixed operations (add, modify, delete, rename in single changeset)
- Binary file detection and diff ignoring
- Lock file detection and diff ignoring

Run tests with: `cargo test git::tests -- --nocapture`

## Project Conventions

- Always use `writeln!(io::stdout(), ...)` instead of `println!` for consistency with error handling and proper stdout buffering
- All code comments should be lowercase per project style
- When testing git functionality, create temporary test repositories using `tempfile::TempDir`
