# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`git-auto-commit` is a Rust CLI tool that analyses git changes and displays files touched with their change types (A/M/D). The tool prioritises staged changes, falling back to unstaged changes (including untracked files) if nothing is staged.

## Build and Test

- Build: `cargo build --release` or `make build`
- Run: `cargo run` (uses debug build) or `./target/release/git-auto-commit`
- Test suite: `make test` (runs formatting check, clippy with pedantic warnings, and tests)
- Format: `make format` or `cargo fmt`
- Install: `make install` (copies binary to /usr/local/bin/)

## Code Architecture

The codebase has several modules:

**`src/git.rs`**: Git operations module with hybrid implementation:
- Uses `git2` crate for reading diffs and staging files
- Uses git binary for creating commits (to support commit signing and git hooks)
- Handles all git states: additions (A), modifications (M), deletions (D), renames (R)
- Rename detection is enabled via `DiffFindOptions` - renames and moves show as single operations with status 'R'
- Automatically ignores diffs for: binary files, lock files (*.lock, *-lock.json/yaml), minified files (*.min.js/css)

**`src/changeset.rs`**: Data structures for representing file changes (`FileChange`) and changesets (`ChangeSet`)
- `FileChange` includes: status (A/M/D/R), path, `old_path` (for renames), and `diff_ignored` flag
- Renames (status 'R') include both `old_path` and `path` (new path)
- `diff_ignored` is true for binary files, lock files, and minified files

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

**`src/claude.rs`**: LLM integration for commit message generation
- Uses the `claude` CLI tool (spawns as subprocess) to generate commit descriptions
- Supports single-line and multi-line commit message formats
- Has configurable prompt with strict rules: 72-char limit, lowercase start, no Claude attribution
- Includes "think hard" mode for improved generation quality
- 30-second timeout for LLM generation
- Allows extra user-provided prompt context

**`src/main.rs`**: Main application workflow
1. Checks for staged changes first
2. Falls back to unstaged changes (including untracked files) if nothing staged
3. Generates commit description via Claude LLM
4. Interactive loop with options:
   - YES: accept and commit
   - no: abort
   - reroll: regenerate description (with "think hard" mode)
   - long/short: toggle between multi-line and single-line formats
   - edit: manually edit description (single-line or multi-line based on current format)
   - prompt: add extra context to Claude prompt
5. Auto-rerolls (max 3 attempts) if any line exceeds 72 characters
6. Warns if commit message contains "Claude" or has unexpected newlines
7. Displays up to 10 files in output (with count of remaining files if more)
8. Stages unstaged changes before committing (if applicable)

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
