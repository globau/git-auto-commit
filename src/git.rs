use crate::changeset::{ChangeSet, FileChange};
use crate::fatal;
use git2::{Delta, DiffFindOptions, DiffFormat, DiffOptions, Repository, RepositoryState};
use std::path::Path;

/// sanity check that we're in a git repository and in a good state
pub fn sanity_check() {
    // check we're in a git repository (can be anywhere within the repo)
    let repo = match Repository::discover(".") {
        Ok(repo) => repo,
        Err(e) => fatal!("not in a git repository: {}", e),
    };

    // check we're not in the middle of a git operation
    if repo.state() != RepositoryState::Clean {
        fatal!("repository is in the middle of an operation (merge, rebase, etc)");
    }

    // check we're not on a detached HEAD
    if repo.head_detached().unwrap_or(false) {
        fatal!("repository is in detached HEAD state");
    }
}

/// get changes from the repository
/// checks staged changes first, falls back to unstaged (including untracked files)
/// returns None if no changes found
pub fn get_changes(path: &Path) -> Result<Option<ChangeSet>, String> {
    let repo = Repository::open(path).map_err(|e| format!("failed to open git repository: {e}"))?;

    // try staged changes first
    let staged_diff = create_staged_diff(&repo)?;
    if staged_diff
        .stats()
        .map_err(|e| format!("failed to get diff stats: {e}"))?
        .files_changed()
        > 0
    {
        let files = files_from_git_diff(&staged_diff);
        let diff = format_diff(&staged_diff, &files)?;
        return Ok(Some(ChangeSet {
            files,
            diff,
            is_staged: true,
        }));
    }

    // no staged changes, try unstaged (includes untracked files)
    let unstaged_diff = create_unstaged_diff(&repo)?;
    let files = files_from_git_diff(&unstaged_diff);

    if files.is_empty() {
        return Ok(None);
    }

    let diff = format_diff(&unstaged_diff, &files)?;
    Ok(Some(ChangeSet {
        files,
        diff,
        is_staged: false,
    }))
}

/// extract list of files from a `git2::Diff` using native types
fn files_from_git_diff(diff: &git2::Diff) -> Vec<FileChange> {
    let mut files = Vec::new();

    for delta in diff.deltas() {
        let status = match delta.status() {
            Delta::Deleted => 'D',
            Delta::Modified | Delta::Typechange => 'M',
            Delta::Renamed => 'R',
            Delta::Added | Delta::Copied | Delta::Untracked => 'A',
            _ => continue, // skip ignored, unmodified, etc.
        };

        let (path, old_path) = if delta.status() == Delta::Renamed {
            // for renames, get both old and new paths
            let new_path = delta.new_file().path();
            let old_path = delta.old_file().path();
            (new_path, old_path.map(|p| p.to_string_lossy().to_string()))
        } else if delta.status() == Delta::Deleted {
            (delta.old_file().path(), None)
        } else {
            (delta.new_file().path(), None)
        };

        if let Some(path) = path {
            let path_str = path.to_string_lossy().to_string();

            // check if diff should be ignored (lock files, minified files, binary files)
            let is_binary = delta.new_file().is_binary() || delta.old_file().is_binary();
            let diff_ignored = should_ignore_diff(&path_str) || is_binary;

            files.push(FileChange {
                status,
                path: path_str,
                old_path,
                diff_ignored,
            });
        }
    }

    files
}

/// create a diff object for staged changes
fn create_staged_diff(repo: &Repository) -> Result<git2::Diff<'_>, String> {
    // handle unborn branch (no commits yet) - compare against empty tree
    let tree = match repo.head() {
        Ok(head) => Some(
            head.peel_to_tree()
                .map_err(|e| format!("failed to get tree: {e}"))?,
        ),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) => return Err(format!("failed to get HEAD: {e}")),
    };

    let mut diff = repo
        .diff_tree_to_index(tree.as_ref(), None, None)
        .map_err(|e| format!("failed to create diff: {e}"))?;

    // enable rename detection with lower threshold for better detection
    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true);
    find_opts.rename_threshold(50); // 50% similarity (git default)
    find_opts.copy_threshold(50);
    diff.find_similar(Some(&mut find_opts))
        .map_err(|e| format!("failed to detect renames: {e}"))?;

    Ok(diff)
}

/// create a diff object for unstaged changes
fn create_unstaged_diff(repo: &Repository) -> Result<git2::Diff<'_>, String> {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.show_untracked_content(true);
    let mut diff = repo
        .diff_index_to_workdir(None, Some(&mut opts))
        .map_err(|e| format!("failed to create diff: {e}"))?;

    // enable rename detection with lower threshold for better detection
    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true);
    find_opts.rename_threshold(50); // 50% similarity (git default)
    find_opts.copy_threshold(50);
    diff.find_similar(Some(&mut find_opts))
        .map_err(|e| format!("failed to detect renames: {e}"))?;

    Ok(diff)
}

/// check if file diff should be ignored (lock files, minified files, etc.)
fn should_ignore_diff(path: &str) -> bool {
    // check specific lock file patterns
    let path_lower = path.to_lowercase();

    // lock files - check full filename patterns
    if path_lower.ends_with("-lock.json") || path_lower.ends_with("-lock.yaml") {
        return true;
    }

    // check file extension for .lock files
    if let Some(ext) = Path::new(path).extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        if ext_lower == "lock" {
            return true;
        }
    }

    // minified files - check patterns before extension
    if path_lower.ends_with(".min.js")
        || path_lower.ends_with(".min.css")
        || path_lower.ends_with("-min.js")
        || path_lower.ends_with("-min.css")
    {
        return true;
    }

    false
}

/// format a diff object into unified diff string, skipping ignored files
fn format_diff(diff: &git2::Diff, files: &[FileChange]) -> Result<String, String> {
    let mut output = String::new();
    let mut current_file: Option<String> = None;
    let mut skip_current_file = false;

    diff.print(DiffFormat::Patch, |delta, _hunk, line| {
        let origin = line.origin();

        // check for file header to determine if we should skip this file
        if origin == 'F'
            && let Some(path) = delta.new_file().path()
        {
            let path_str = path.to_string_lossy().to_string();
            current_file = Some(path_str.clone());

            // check if this file should be ignored based on files list
            skip_current_file = files
                .iter()
                .find(|f| f.path == path_str)
                .is_some_and(|f| f.diff_ignored);

            if skip_current_file {
                // add a note that this file's diff was ignored
                use std::fmt::Write;
                let _ = writeln!(output, "--- {path_str} (diff ignored)");
                return true;
            }
        }

        // skip content if current file is ignored
        if skip_current_file {
            return true;
        }

        let content = std::str::from_utf8(line.content()).unwrap_or("");

        match origin {
            // diff line types that need the origin character
            '+' | '-' | ' ' => {
                output.push(origin);
            }
            // other origin types (headers, etc.) don't need the character
            _ => {}
        }
        output.push_str(content);
        true
    })
    .map_err(|e| format!("failed to format diff: {e}"))?;

    Ok(output.trim_end_matches('\n').to_string())
}

/// stage all files in the changeset
pub fn stage(path: &Path, changeset: &ChangeSet) {
    let repo = match Repository::open(path) {
        Ok(repo) => repo,
        Err(e) => fatal!("failed to open git repository: {}", e),
    };
    let mut index = match repo.index() {
        Ok(index) => index,
        Err(e) => fatal!("failed to get git index: {}", e),
    };

    // collect all errors before writing index
    let mut errors = Vec::new();

    // stage each file according to its status
    for file in &changeset.files {
        let path = &file.path;
        match file.status {
            'D' => {
                // deletions: remove from index
                if let Err(e) = index.remove_path(Path::new(path)) {
                    errors.push(format!("failed to stage deletion of {path}: {e}"));
                }
            }
            'R' => {
                // renames: remove old path and add new path
                let old_path = file
                    .old_path
                    .as_ref()
                    .expect("rename operation must have old_path");
                if let Err(e) = index.remove_path(Path::new(old_path)) {
                    errors.push(format!("failed to remove old path {old_path}: {e}"));
                } else if let Err(e) = index.add_path(Path::new(path)) {
                    errors.push(format!("failed to stage rename to {path}: {e}"));
                }
            }
            'A' | 'M' => {
                // additions and modifications: add to index
                if let Err(e) = index.add_path(Path::new(path)) {
                    errors.push(format!("failed to stage {path}: {e}"));
                }
            }
            status => {
                errors.push(format!("unexpected file status: {status}"));
            }
        }
    }

    // if there were any errors, reload index to rollback and report errors
    if !errors.is_empty() {
        // rollback by reloading from disk
        if let Err(e) = index.read(false) {
            crate::warning!("failed to reload index during rollback: {}", e);
        }
        for error in &errors {
            crate::error!("{}", error);
        }
        fatal!("failed to stage files");
    }

    // write the index to disk
    if let Err(e) = index.write() {
        fatal!("failed to write git index: {}", e);
    }
}

/// create a commit with the given message
///
/// uses the git binary rather than git2 to ensure commit signing (gpg/ssh)
/// and git hooks (pre-commit, commit-msg, etc.) work as expected
pub fn commit(path: &Path, commit_description: &str) {
    let status = std::process::Command::new("git")
        .arg("commit")
        .arg("--message")
        .arg(commit_description)
        .current_dir(path)
        .status();

    match status {
        Ok(exit_status) if exit_status.success() => {}
        Ok(exit_status) => {
            fatal!("git commit failed with exit code: {}", exit_status);
        }
        Err(e) => {
            fatal!("failed to run git commit: {}", e);
        }
    }
}

#[cfg(test)]
mod tests;
