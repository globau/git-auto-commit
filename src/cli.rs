use clap::Parser;

/// git-auto-commit: analyse git changes and display files touched with their change types
#[derive(Parser, Debug)]
#[command(
    name = "git-auto-commit",
    about,
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// print the prompt sent to claude
    #[arg(long)]
    pub debug_prompt: bool,

    /// print the full JSON response from claude
    #[arg(long)]
    pub debug_response: bool,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
