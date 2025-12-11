# git-auto-commit

A Rust CLI tool that analyses git changes and generates commit messages using Claude AI.

## Overview

`git-auto-commit` streamlines your git workflow by:
- Automatically detecting which files have changed (staged or unstaged)
- Generating intelligent commit descriptions using Claude LLM
- Providing an interactive interface to accept, edit, or regenerate commit messages
- Supporting both single-line and multi-line commit formats

## Features

- **Smart change detection**: Prioritises staged changes, falls back to unstaged changes (including untracked files) if nothing is staged
- **AI-powered commit messages**: Uses the `claude` CLI tool to generate contextual commit descriptions
- **Adaptive model selection**: Starts with fast model (Haiku) for initial generation, automatically switches to smart model (Sonnet) for rerolls
- **Ultrathink mode**: Enhanced generation quality after 3+ consecutive manual rerolls
- **Token usage transparency**: Displays token count and USD cost for each generation
- **Rename detection**: Correctly identifies file moves and renames as single operations
- **Interactive workflow**: Accept, edit, reroll, or add context to generated messages
- **Format flexibility**: Toggle between single-line and multi-line commit message formats
- **Large diff handling**: Warns at 50KB, reduces context, and limits maximum diff size to 100KB
- **Diff filtering**: Automatically skips diffs for binaries, lock files (*.lock, *-lock.json/yaml), and minified files (*.min.js/css, *-min.js/css)

## Requirements

- Rust 1.88 or later
- Git
- [`claude` CLI tool](https://github.com/anthropics/claude-cli) - required for commit message generation

## Installation

### From source

```bash
git clone https://github.com/globau/git-auto-commit.git
cd git-auto-commit
make install
```

This builds the release binary and installs it into Cargo's `bin` directory.

Alternatively, build manually:

```bash
cargo build --release
cp target/release/git-auto-commit /usr/local/bin/
```

**Note**: The binary must be named `git-auto-commit` and placed in a directory on your PATH (such as `/usr/local/bin/`) for the `git auto-commit` subcommand syntax to work.

## Usage

Navigate to any git repository and run:

```bash
git auto-commit
```

The tool will:

1. Detect your changes (staged first, then unstaged if nothing is staged)
2. Generate a commit description using Claude
3. Present an interactive prompt with options:
   - **[Y]ES** - Accept the commit message and create the commit
   - **[n]o** - Abort without committing
   - **[r]eroll** - Regenerate the description (with enhanced "think hard" mode)
   - **[l]ong** / **[s]hort** - Toggle between multi-line and single-line formats
   - **[e]dit** - Manually edit the commit message
   - **[p]rompt** - Add extra context to guide Claude's generation

### Example session

```
$ git auto-commit
staged changes [3 files] (2,145 tokens, $0.0016 USD)

add user authentication with JWT tokens

files:
A src/auth.rs
M src/main.rs
M Cargo.toml

[Y]ES/[n]o/[r]eroll/[l]ong/[e]dit/[p]rompt ? long
staged changes [3 files] (2,834 tokens, $0.0021 USD)

add user authentication with JWT tokens

Implement JWT-based authentication system with login and token
validation endpoints. Update dependencies to include jsonwebtoken
crate.

files:
A src/auth.rs
M src/main.rs
M Cargo.toml

[Y]ES/[n]o/[r]eroll/[s]hort/[e]dit/[p]rompt ? YES

[main abc1234] add user authentication with JWT tokens
 3 files changed, 156 insertions(+), 2 deletions(-)
 create mode 100644 src/auth.rs
```

## CLI Options

```bash
git auto-commit [OPTIONS]
```

**Options:**
- `--debug-prompt` - Display the full prompt sent to Claude (useful for debugging or understanding generation behaviour)
- `--debug-response` - Display the full JSON response from Claude (useful for debugging)

## Commit message rules

Generated commit messages follow these rules:
- Maximum 72 characters per line
- Start with lowercase letter
- No Claude attribution or metadata
- Focus on outcome, not implementation details

## Development

### Building

```bash
cargo build          # debug build
cargo build --release # optimised build
make build           # same as cargo build --release
```

### Testing

```bash
make test            # runs format check, clippy, and tests
cargo test           # run tests only
cargo test git::tests -- --nocapture # run git tests with output
```

### Code formatting

```bash
make format          # or cargo fmt
```

## Architecture

The codebase is organised into several modules:

- **`src/git.rs`** - Git operations using hybrid approach (`git2` crate for diffs, git binary for commits), includes data structures for file changes and changesets
- **`src/ui.rs`** - User interface utilities (prompts, editors, output macros)
- **`src/claude.rs`** - LLM integration for commit message generation with model selection and token tracking
- **`src/context.rs`** - Application state management bundling all mutable state
- **`src/cli.rs`** - Command-line argument parsing
- **`src/constants.rs`** - Configuration constants (line length limits, timeouts, model names, thresholds)
- **`src/main.rs`** - Main application workflow and interactive loop

## Licence

See LICENCE file for details.
