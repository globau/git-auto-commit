/// application context holding state throughout the commit generation workflow
#[allow(clippy::struct_excessive_bools)]
pub struct AppContext {
    /// the current commit description
    pub commit_description: String,

    /// whether to generate multi-line commit messages
    pub multi_line: bool,

    /// number of context lines for git unified diff
    pub context_lines: u32,

    /// whether to enable "think hard" mode for generation
    pub think_hard: bool,

    /// extra user-provided context for the prompt
    pub prompt_extra: String,

    /// whether to regenerate the commit description on next iteration
    pub regenerate: bool,

    /// count of automatic rerolls (for line length violations)
    pub auto_reroll_count: usize,

    /// whether the user has manually edited the commit description
    pub user_edited: bool,

    /// whether to show the debug prompt (from --debug-prompt flag)
    pub show_prompt: bool,
}

impl AppContext {
    /// create a new context with default values
    pub fn new(show_prompt: bool) -> Self {
        Self {
            // commit desc
            commit_description: String::from("bug fixes and/or improvements"),
            multi_line: false,
            // prompt
            context_lines: crate::constants::DEFAULT_CONTEXT,
            think_hard: false,
            prompt_extra: String::new(),
            // state
            regenerate: true,
            auto_reroll_count: 0,
            user_edited: false,
            // debugging
            show_prompt,
        }
    }
}
