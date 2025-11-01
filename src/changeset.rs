/// represents a single file change with its status
#[derive(Debug)]
pub struct FileChange {
    pub status: char, // 'A', 'M', 'D', or 'R'
    pub path: String,
    pub old_path: Option<String>, // set for renames ('R' status)
    pub diff_ignored: bool,       // lock files, minified files, etc.
}

/// represents a set of changes (staged or unstaged)
#[derive(Debug)]
pub struct ChangeSet {
    pub files: Vec<FileChange>,
    pub diff: String,
    pub is_staged: bool,
}

impl ChangeSet {
    pub fn source(&self) -> &str {
        if self.is_staged {
            "staged changes"
        } else {
            "unstaged changes"
        }
    }
}
