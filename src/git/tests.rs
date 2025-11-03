use super::*;
use git2::Delta;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// helper to initialise a test git repository
fn setup_test_repo() -> (TempDir, Repository) {
    let temp_dir = TempDir::new().unwrap();
    let repo = Repository::init(temp_dir.path()).unwrap();

    // configure git user for commits
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();

    (temp_dir, repo)
}

/// helper to create a file with content
fn create_file(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

/// helper to commit all changes
fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();

    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = repo.signature().unwrap();

    let parent_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    if let Some(parent) = parent_commit {
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent],
        )
        .unwrap();
    } else {
        // first commit
        repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
            .unwrap();
    }
}

#[test]
fn test_file_rename() {
    let (temp_dir, repo) = setup_test_repo();
    let repo_path = temp_dir.path();

    // create and commit initial file
    create_file(&repo_path.join("old_name.txt"), "file content");
    commit_all(&repo, "initial commit");

    // rename file
    fs::rename(
        repo_path.join("old_name.txt"),
        repo_path.join("new_name.txt"),
    )
    .unwrap();

    // stage the rename
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("old_name.txt")).unwrap();
    index.add_path(Path::new("new_name.txt")).unwrap();
    index.write().unwrap();

    // get changes - should detect rename
    let changes = get_changes(repo_path).unwrap();

    assert!(changes.is_some());
    let changeset = changes.unwrap();

    // should detect rename as single operation
    assert_eq!(
        changeset.files.len(),
        1,
        "rename detected as single operation"
    );

    let file = &changeset.files[0];
    assert_eq!(file.status, Delta::Renamed, "status should be R for rename");
    assert_eq!(file.path, "new_name.txt");
    assert_eq!(file.old_path, Some("old_name.txt".to_string()));

    println!("Rename detected:");
    println!(
        "  status: {:?}, old_path: {:?}, new_path: {}",
        file.status, file.old_path, file.path
    );
}

#[test]
fn test_file_move_to_subdirectory() {
    let (temp_dir, repo) = setup_test_repo();
    let repo_path = temp_dir.path();

    // create and commit initial file
    create_file(&repo_path.join("file.txt"), "content");
    commit_all(&repo, "initial commit");

    // create subdirectory and move file
    fs::create_dir(repo_path.join("subdir")).unwrap();
    fs::rename(
        repo_path.join("file.txt"),
        repo_path.join("subdir/file.txt"),
    )
    .unwrap();

    // stage the move
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("file.txt")).unwrap();
    index.add_path(Path::new("subdir/file.txt")).unwrap();
    index.write().unwrap();

    // get changes
    let changes = get_changes(repo_path).unwrap();

    assert!(changes.is_some());
    let changeset = changes.unwrap();

    // should detect move as single rename operation
    assert_eq!(
        changeset.files.len(),
        1,
        "move detected as single rename operation"
    );

    let file = &changeset.files[0];
    assert_eq!(file.status, Delta::Renamed, "status should be R for move");
    assert_eq!(file.path, "subdir/file.txt");
    assert_eq!(file.old_path, Some("file.txt".to_string()));

    println!("Move detected:");
    println!(
        "  status: {:?}, old_path: {:?}, new_path: {}",
        file.status, file.old_path, file.path
    );
}

#[test]
fn test_mixed_operations() {
    let (temp_dir, repo) = setup_test_repo();
    let repo_path = temp_dir.path();

    // create and commit initial files
    create_file(&repo_path.join("to_modify.txt"), "original");
    create_file(&repo_path.join("to_delete.txt"), "delete me");
    create_file(&repo_path.join("to_rename.txt"), "rename me");
    commit_all(&repo, "initial commit");

    // perform mixed operations
    create_file(&repo_path.join("to_modify.txt"), "modified"); // modify
    fs::remove_file(repo_path.join("to_delete.txt")).unwrap(); // delete
    fs::rename(
        repo_path.join("to_rename.txt"),
        repo_path.join("renamed.txt"),
    )
    .unwrap(); // rename
    create_file(&repo_path.join("new_file.txt"), "new"); // add

    // stage all changes
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.remove_path(Path::new("to_delete.txt")).unwrap();
    index.remove_path(Path::new("to_rename.txt")).unwrap();
    index.write().unwrap();

    // get changes
    let changes = get_changes(repo_path).unwrap();

    assert!(changes.is_some());
    let changeset = changes.unwrap();
    println!(
        "Mixed operations - {} file(s) changed:",
        changeset.files.len()
    );
    for file in &changeset.files {
        if let Some(old_path) = &file.old_path {
            println!("  status: {:?}, {} → {}", file.status, old_path, file.path);
        } else {
            println!("  status: {:?}, path: {}", file.status, file.path);
        }
    }

    // we expect 4 files: modified, deleted, renamed (as single R), added
    assert_eq!(
        changeset.files.len(),
        4,
        "should have 4 file changes (M, D, R, A)"
    );
}

#[test]
fn test_binary_file_is_ignored() {
    let (temp_dir, repo) = setup_test_repo();
    let repo_path = temp_dir.path();

    // create binary file
    let binary_content = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
    fs::write(repo_path.join("data.bin"), binary_content).unwrap();

    // create text file
    create_file(&repo_path.join("text.txt"), "text content");

    // stage files
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();

    // get changes
    let changes = get_changes(repo_path).unwrap();

    assert!(changes.is_some());
    let changeset = changes.unwrap();

    println!("Binary file test - {} file(s):", changeset.files.len());
    for file in &changeset.files {
        println!(
            "  status: {:?}, path: {}, diff_ignored: {}",
            file.status, file.path, file.diff_ignored
        );
    }

    // find the binary file
    let binary_file = changeset
        .files
        .iter()
        .find(|f| f.path == "data.bin")
        .expect("binary file should be in changes");

    assert!(
        binary_file.diff_ignored,
        "binary file should have diff_ignored = true"
    );

    // find text file
    let text_file = changeset
        .files
        .iter()
        .find(|f| f.path == "text.txt")
        .expect("text file should be in changes");

    assert!(
        !text_file.diff_ignored,
        "text file should not have diff_ignored = true"
    );
}

#[test]
fn test_lock_file_is_ignored() {
    let (temp_dir, repo) = setup_test_repo();
    let repo_path = temp_dir.path();

    // create lock file and normal file
    create_file(&repo_path.join("Cargo.lock"), "lock content");
    create_file(&repo_path.join("src.rs"), "code content");

    // stage files
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();

    // get changes
    let changes = get_changes(repo_path).unwrap();

    assert!(changes.is_some());
    let changeset = changes.unwrap();

    // find the lock file
    let lock_file = changeset
        .files
        .iter()
        .find(|f| f.path == "Cargo.lock")
        .expect("lock file should be in changes");

    assert!(
        lock_file.diff_ignored,
        "lock file should have diff_ignored = true"
    );

    // find normal file
    let normal_file = changeset
        .files
        .iter()
        .find(|f| f.path == "src.rs")
        .expect("normal file should be in changes");

    assert!(
        !normal_file.diff_ignored,
        "normal file should not have diff_ignored = true"
    );
}

#[test]
fn test_stage_function_with_deletions_and_renames() {
    let (temp_dir, repo) = setup_test_repo();
    let repo_path = temp_dir.path();

    // create and commit initial files
    create_file(&repo_path.join("to_modify.txt"), "original");
    create_file(&repo_path.join("to_delete.txt"), "delete me");
    create_file(&repo_path.join("to_rename.txt"), "rename me");
    commit_all(&repo, "initial commit");

    // perform mixed operations (unstaged)
    create_file(&repo_path.join("to_modify.txt"), "modified"); // modify
    fs::remove_file(repo_path.join("to_delete.txt")).unwrap(); // delete
    fs::rename(
        repo_path.join("to_rename.txt"),
        repo_path.join("renamed.txt"),
    )
    .unwrap(); // rename
    create_file(&repo_path.join("new_file.txt"), "new"); // add

    // get unstaged changes
    let changes = get_changes(repo_path).unwrap();
    assert!(changes.is_some());
    let changeset = changes.unwrap();
    assert!(!changeset.is_staged, "changes should be unstaged");

    println!(
        "Before staging - {} unstaged file(s):",
        changeset.files.len()
    );
    for file in &changeset.files {
        println!("  status: {:?}, path: {}", file.status, file.path);
    }

    // stage the changes using our stage() function
    stage(repo_path, &changeset);

    // verify all changes are now staged
    let staged_diff = create_staged_diff(&repo).unwrap();
    let staged_files = files_from_git_diff(&staged_diff);

    println!("After staging - {} staged file(s):", staged_files.len());
    for file in &staged_files {
        if let Some(old_path) = &file.old_path {
            println!("  status: {:?}, {} → {}", file.status, old_path, file.path);
        } else {
            println!("  status: {:?}, path: {}", file.status, file.path);
        }
    }

    // verify we have the expected changes staged
    assert_eq!(
        staged_files.len(),
        4,
        "should have 4 staged changes (M, D, R, A)"
    );

    // verify each operation is correctly staged
    let has_modified = staged_files
        .iter()
        .any(|f| f.status == Delta::Modified && f.path == "to_modify.txt");
    let has_deleted = staged_files
        .iter()
        .any(|f| f.status == Delta::Deleted && f.path == "to_delete.txt");
    let has_renamed = staged_files.iter().any(|f| {
        f.status == Delta::Renamed
            && f.path == "renamed.txt"
            && f.old_path == Some("to_rename.txt".to_string())
    });
    let has_added = staged_files
        .iter()
        .any(|f| matches!(f.status, Delta::Added | Delta::Untracked) && f.path == "new_file.txt");

    assert!(has_modified, "modified file should be staged");
    assert!(has_deleted, "deleted file should be staged");
    assert!(has_renamed, "renamed file should be staged as rename");
    assert!(has_added, "new file should be staged");
}
