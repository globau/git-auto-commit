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
    // no arguments yet, but --help is provided by clap
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
