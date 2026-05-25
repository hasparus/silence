use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn silence_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_silence"))
}

pub fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn run_silence(dir: &Path, args: &[&str], extra_env: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(silence_bin());
    cmd.args(args).current_dir(dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("the silence binary should run")
}

pub fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be installed");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub struct Repo {
    pub dir: PathBuf,
}

impl Repo {
    pub fn new(name: &str) -> Repo {
        let dir = tmp(name);
        let repo = Repo { dir };
        git(&repo.dir, &["init", "-q"]);
        git(&repo.dir, &["config", "user.email", "test@example.com"]);
        git(&repo.dir, &["config", "user.name", "test"]);
        git(&repo.dir, &["config", "commit.gpgsign", "false"]);
        repo
    }

    pub fn git(&self, args: &[&str]) {
        git(&self.dir, args);
    }

    pub fn git_output(&self, args: &[&str]) -> Output {
        Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("git must be installed")
    }

    pub fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.dir.join(name), contents).unwrap();
    }

    pub fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.join(name)).unwrap()
    }

    pub fn silence(&self, args: &[&str]) -> Output {
        run_silence(&self.dir, args, &[])
    }

    pub fn commit_baseline(&self, name: &str, contents: &str) {
        self.write(name, contents);
        self.git(&["add", "."]);
        self.git(&["commit", "-q", "-m", "baseline"]);
    }
}
