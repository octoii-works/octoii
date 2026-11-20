//! Linked worktrees are prepared on the caller's machine, from a fixed commit, without moving its uncommitted work.

use std::path::Path;
use std::process::Command;

use octoii::machine::{Base, Host, Machine, Writes};
use octoii::testing::Calling;
use siphon_git::git::{Git, GitError, Worktree};
use tokio_util::sync::CancellationToken;

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git's text")
        .trim_end_matches('\n')
        .to_owned()
}

fn repository(root: &Path) -> String {
    git(root, &["init"]);
    std::fs::write(root.join("tracked.txt"), "committed\n").expect("written");
    git(root, &["add", "tracked.txt"]);
    git(root, &["commit", "-m", "base"]);
    git(root, &["rev-parse", "HEAD"])
}

struct Worktrees {
    calling: Calling,
    machine: Machine,
}

impl Worktrees {
    fn start(root: &Path, writes: &Writes) -> Self {
        let calling = Calling::start(|tools| tools);
        let machine = calling
            .volume()
            .commands()
            .open(
                &Host::default(),
                Base {
                    workspace: root.to_owned(),
                    home: None,
                },
                writes,
            )
            .expect("the caller's machine");
        Self { calling, machine }
    }

    async fn create(&self, folder: &Path, destination: &Path, branch: &str, base: &str) -> Result<Worktree, GitError> {
        Git {
            machine: &self.machine,
            commands: &self.calling.volume().commands(),
            cancelled: CancellationToken::new(),
        }
        .create_worktree(folder, destination, branch, base)
        .await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn worktrees_start_at_a_fixed_commit_and_keep_their_work_separate() {
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let root = workspace.join("source");
    std::fs::create_dir(&root).expect("the source folder");
    let base = repository(&root);
    git(&root, &["tag", "-a", "baseline", "-m", "the base"]);
    git(&root, &["checkout", "-b", "source"]);
    std::fs::write(root.join("later.txt"), "a later commit\n").expect("written");
    git(&root, &["add", "later.txt"]);
    git(&root, &["commit", "-m", "later"]);
    std::fs::write(root.join("tracked.txt"), "staged\n").expect("written");
    git(&root, &["add", "tracked.txt"]);
    std::fs::write(root.join("tracked.txt"), "unstaged\n").expect("written");
    std::fs::write(root.join("untracked.txt"), "untracked\n").expect("written");
    let status = git(&root, &["status", "--porcelain"]);
    let head = git(&root, &["rev-parse", "HEAD"]);
    let worktrees = Worktrees::start(&workspace, &Writes::Within(vec![workspace.clone()]));
    let first_path = workspace.join("--first $(touch${IFS}path-ran)");
    let first_branch = "topic/$(touch${IFS}branch-ran)";
    let first = worktrees
        .create(&root, &first_path, first_branch, "baseline")
        .await
        .expect("first worktree");
    let second_path = workspace.join("second worktree");
    let second = worktrees
        .create(&root, &second_path, "topic/second", "baseline")
        .await
        .expect("second worktree");
    assert_eq!(
        first,
        Worktree {
            path: first_path.to_str().expect("a path").to_owned(),
            branch: first_branch.to_owned(),
            base_commit: base.clone(),
        }
    );
    assert_eq!(second.base_commit, base);
    for (path, branch) in [(&first_path, first_branch), (&second_path, "topic/second")] {
        assert_eq!(git(path, &["rev-parse", "HEAD"]), base);
        assert_eq!(git(path, &["symbolic-ref", "--short", "HEAD"]), branch);
        assert_eq!(git(path, &["status", "--porcelain"]), "");
        assert_eq!(std::fs::read_to_string(path.join("tracked.txt")).expect("read"), "committed\n");
        assert!(!path.join("later.txt").exists());
        assert!(!path.join("untracked.txt").exists());
    }
    std::fs::write(first_path.join("tracked.txt"), "first worktree\n").expect("written");
    std::fs::write(second_path.join("tracked.txt"), "second worktree\n").expect("written");
    assert_eq!(
        std::fs::read_to_string(first_path.join("tracked.txt")).expect("read"),
        "first worktree\n"
    );
    assert_eq!(
        std::fs::read_to_string(second_path.join("tracked.txt")).expect("read"),
        "second worktree\n"
    );
    assert_eq!(std::fs::read_to_string(root.join("tracked.txt")).expect("read"), "unstaged\n");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&root, &["symbolic-ref", "--short", "HEAD"]), "source");
    assert_eq!(git(&root, &["show", ":tracked.txt"]), "staged");
    assert_eq!(git(&root, &["status", "--porcelain"]), status);
    assert!(!root.join("path-ran").exists());
    assert!(!root.join("branch-ran").exists());
    worktrees.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_branches_and_paths_are_rejected_and_preserved() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let base = repository(&root);
    git(&root, &["branch", "taken"]);
    let worktrees = Worktrees::start(&root, &Writes::Within(vec![root.clone()]));
    let unused = root.join("unused");
    let error = worktrees
        .create(&root, &unused, "taken", "HEAD")
        .await
        .expect_err("an existing branch");
    assert!(error.to_string().contains(unused.to_str().expect("a path")));
    assert!(error.to_string().contains("taken"));
    assert!(!unused.exists());
    assert_eq!(git(&root, &["rev-parse", "taken"]), base);
    let empty = root.join("empty");
    std::fs::create_dir(&empty).expect("an empty folder");
    let occupied = root.join("occupied");
    std::fs::create_dir(&occupied).expect("a folder");
    std::fs::write(occupied.join("keep.txt"), "keep\n").expect("written");
    let file = root.join("file");
    std::fs::write(&file, "a file\n").expect("written");
    for (index, destination) in [&occupied, &file].into_iter().enumerate() {
        let branch = format!("new-branch-{index}");
        let error = worktrees
            .create(&root, destination, &branch, "HEAD")
            .await
            .expect_err("an existing path");
        assert!(error.to_string().contains(destination.to_str().expect("a path")));
        assert!(error.to_string().contains("new-branch"));
    }
    assert_eq!(std::fs::read_dir(&empty).expect("read").count(), 0);
    assert_eq!(std::fs::read_to_string(occupied.join("keep.txt")).expect("read"), "keep\n");
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "a file\n");
    // Git may create the new branch before refusing the directory; never roll it back ourselves.
    assert_eq!(git(&root, &["rev-parse", "main"]), base);
    assert_eq!(git(&root, &["rev-parse", "taken"]), base);
    worktrees.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_branches_and_bases_never_create_a_destination() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    repository(&root);
    git(&root, &["checkout", "-b", "source"]);
    let worktrees = Worktrees::start(&root, &Writes::Within(vec![root.clone()]));
    let destination = root.join("new worktree");
    let branches = git(&root, &["for-each-ref", "--format=%(refname):%(objectname)", "refs/heads/"]);
    for branch in [
        "",
        "--force",
        "-B",
        "HEAD",
        "@{-1}",
        "topic@{-1}",
        "../escape",
        "with space",
        "topic.lock",
    ] {
        worktrees
            .create(&root, &destination, branch, "HEAD")
            .await
            .expect_err("an invalid literal branch");
        assert!(!destination.exists(), "branch {branch}");
    }
    for base in [
        "",
        "--all",
        "--output-object-format=sha256",
        "missing",
        "HEAD^{tree}",
        "HEAD:tracked.txt",
    ] {
        worktrees
            .create(&root, &destination, "valid", base)
            .await
            .expect_err("an invalid commit base");
        assert!(!destination.exists(), "base {base}");
    }
    worktrees
        .create(&root, Path::new("--relative"), "valid", "HEAD")
        .await
        .expect_err("an absolute destination");
    assert_eq!(
        git(&root, &["for-each-ref", "--format=%(refname):%(objectname)", "refs/heads/"]),
        branches
    );
    worktrees.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_git_failure_preserves_the_existing_worktree_without_reserving_a_directory() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    let base = repository(&root);
    let destination = root.join("registered");
    git(
        &root,
        &["worktree", "add", "-b", "existing", destination.to_str().expect("a path"), "HEAD"],
    );
    std::fs::write(destination.join("keep.txt"), "existing work\n").expect("written");
    let moved = root.join("moved");
    std::fs::rename(&destination, &moved).expect("left a registered path absent");
    let worktrees = Worktrees::start(&root, &Writes::Within(vec![root.clone()]));
    let error = worktrees
        .create(&root, &destination, "new-branch", "HEAD")
        .await
        .expect_err("still registered");
    assert!(error.to_string().contains(destination.to_str().expect("a path")));
    assert!(error.to_string().contains("new-branch"));
    assert!(!destination.exists());
    assert_eq!(std::fs::read_to_string(moved.join("keep.txt")).expect("read"), "existing work\n");
    assert_eq!(git(&root, &["rev-parse", "existing"]), base);
    worktrees.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn preparation_does_not_expand_the_callers_write_permissions() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    repository(&root);
    let branches = git(&root, &["for-each-ref", "--format=%(refname):%(objectname)", "refs/heads/"]);
    let worktrees = Worktrees::start(&root, &Writes::Within(Vec::new()));
    let destination = root.join("denied");
    let error = worktrees
        .create(&root, &destination, "denied", "HEAD")
        .await
        .expect_err("read-only machine");
    assert!(error.to_string().contains(destination.to_str().expect("a path")));
    assert!(!destination.exists());
    assert_eq!(
        git(&root, &["for-each-ref", "--format=%(refname):%(objectname)", "refs/heads/"]),
        branches
    );
    worktrees.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_worktree_actions_keep_the_workspace_and_protect_uncommitted_work() {
    use octoii::tools::{CallStatus, Tool};
    use serde_json::{Value, json};
    use siphon_git::tool::GitTool;

    let folder = tempfile::tempdir().unwrap();
    let workspace = folder.path().canonicalize().unwrap();
    let root = workspace.join("source");
    std::fs::create_dir(&root).unwrap();
    repository(&root);
    let calling = Calling::start(|tools| tools.with(GitTool));
    let schema = GitTool::output_schema().unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let call = async |key: &str, args: Value| {
        calling
            .called(key, root.clone(), Vec::new(), vec![(key.into(), "git".into(), args.to_string())])
            .await
            .remove(0)
    };
    let created = call(
        "create",
        json!({"action": "create_worktree", "path": "../topic\nwith space", "branch": "topic"}),
    )
    .await;
    assert_eq!(created.status, CallStatus::Ok, "{}", created.text);
    assert!(validator.is_valid(created.structured_content.as_ref().unwrap()));
    let destination = workspace.join("topic\nwith space");
    assert!(destination.join("tracked.txt").exists());
    let status = call("status", json!({"action": "status"})).await;
    assert_eq!(
        status.structured_content.as_ref().unwrap()["repository"]["root"],
        root.to_str().unwrap()
    );
    assert!(validator.is_valid(status.structured_content.as_ref().unwrap()));
    git(
        &root,
        &["worktree", "lock", "--reason", "keep\nthis", destination.to_str().unwrap()],
    );
    let listed = call("list", json!({"action": "list_worktrees"})).await;
    let data = listed.structured_content.unwrap();
    assert!(validator.is_valid(&data));
    let entry = data["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["branch"] == "topic")
        .unwrap();
    assert_eq!(entry["path"], destination.to_str().unwrap());
    assert_eq!(entry["locked"], "keep\nthis");
    let remove = json!({"action": "remove_worktree", "path": destination});
    assert_eq!(call("locked", remove.clone()).await.status, CallStatus::ToolError);
    git(&root, &["worktree", "unlock", destination.to_str().unwrap()]);
    std::fs::write(destination.join("untracked"), "keep").unwrap();
    assert_eq!(call("dirty", remove.clone()).await.status, CallStatus::ToolError);
    assert!(destination.join("untracked").exists());
    std::fs::remove_file(destination.join("untracked")).unwrap();
    let removed = call("remove", remove).await;
    assert_eq!(removed.status, CallStatus::Ok, "{}", removed.text);
    assert!(validator.is_valid(removed.structured_content.as_ref().unwrap()));
    assert!(!destination.exists());
    assert_eq!(git(&root, &["rev-parse", "refs/heads/topic"]), git(&root, &["rev-parse", "HEAD"]));
    assert_eq!(
        call("main", json!({"action": "remove_worktree", "path": "."})).await.status,
        CallStatus::ToolError
    );
    assert!(root.join("tracked.txt").exists());
    calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn creation_uses_git_directory_and_remote_tracking_semantics() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let commit = repository(&root);
    git(&root, &["remote", "add", "origin", "."]);
    git(&root, &["update-ref", "refs/remotes/origin/main", &commit]);
    git(&root, &["config", "branch.autoSetupMerge", "true"]);
    let worktrees = Worktrees::start(&root, &Writes::Within(vec![root.clone()]));
    let empty = root.join("empty");
    std::fs::create_dir(&empty).unwrap();
    let created = worktrees.create(&root, &empty, "tracked", "origin/main").await.unwrap();
    assert_eq!(created.base_commit, commit);
    assert_eq!(git(&empty, &["rev-parse", "--abbrev-ref", "@{upstream}"]), "origin/main");
    let nested = root.join("missing/parent/worktree");
    worktrees.create(&root, &nested, "nested", "HEAD").await.unwrap();
    assert!(nested.join("tracked.txt").exists());
    worktrees.calling.close().await;
}
