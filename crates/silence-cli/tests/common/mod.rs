use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub type TestResult = Result<(), Box<dyn Error>>;
pub type TResult<T> = Result<T, Box<dyn Error>>;

pub fn silence_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_silence"))
}

pub fn tmp(name: &str) -> TResult<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[allow(dead_code)]
pub fn tmp_outside_repo(name: &str) -> TResult<PathBuf> {
    let dir = std::env::temp_dir().join(format!("silence-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let discovery = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&dir)
        .output()?;
    assert!(
        !discovery.status.success(),
        "temporary directory is inside git"
    );
    Ok(dir)
}

pub fn run_silence(dir: &Path, args: &[&str], extra_env: &[(&str, &Path)]) -> TResult<Output> {
    let mut cmd = Command::new(silence_bin());
    cmd.args(args).current_dir(dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    Ok(cmd.output()?)
}

#[allow(dead_code)]
pub fn run_hook_stdin(dir: &Path, payload: &str) -> TResult<Output> {
    use std::io::Write;

    let mut child = Command::new(silence_bin())
        .args(["hook"])
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or("child stdin must be open")?
        .write_all(payload.as_bytes())?;
    Ok(child.wait_with_output()?)
}

pub fn git(dir: &Path, args: &[&str]) -> TResult<()> {
    let out = Command::new("git").args(args).current_dir(dir).output()?;
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[allow(dead_code)]
pub struct Repo {
    pub dir: PathBuf,
}

#[allow(dead_code)]
impl Repo {
    pub fn new(name: &str) -> TResult<Repo> {
        let dir = tmp(name)?;
        let repo = Repo { dir };
        git(&repo.dir, &["init", "-q"])?;
        git(&repo.dir, &["config", "user.email", "test@example.com"])?;
        git(&repo.dir, &["config", "user.name", "test"])?;
        git(&repo.dir, &["config", "commit.gpgsign", "false"])?;
        Ok(repo)
    }

    pub fn git(&self, args: &[&str]) -> TResult<()> {
        git(&self.dir, args)
    }

    pub fn git_output(&self, args: &[&str]) -> TResult<Output> {
        Ok(Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()?)
    }

    pub fn write(&self, name: &str, contents: &str) -> TResult<()> {
        std::fs::write(self.dir.join(name), contents)?;
        Ok(())
    }

    pub fn read(&self, name: &str) -> TResult<String> {
        Ok(std::fs::read_to_string(self.dir.join(name))?)
    }

    pub fn silence(&self, args: &[&str]) -> TResult<Output> {
        run_silence(&self.dir, args, &[])
    }

    pub fn commit_baseline(&self, name: &str, contents: &str) -> TResult<()> {
        self.write(name, contents)?;
        self.git(&["add", "."])?;
        self.git(&["commit", "-q", "-m", "baseline"])?;
        Ok(())
    }
}
