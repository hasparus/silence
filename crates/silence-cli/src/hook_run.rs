use serde_json::json;
use silence_core::{CommentKinds, LineMode, Lines, PreserveConfig};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::git;
use crate::hook_input::{self, HookJob};
use crate::strip::{lang_for, strip_file, StripOpts, StripOutcome, WriteMode};

enum HookSkip {
    NotAFile(PathBuf),
    NotInGitChanges(PathBuf),
    UnsupportedLang(PathBuf),
    StripFailed(PathBuf, String),
    OutsideRepo(PathBuf),
}

/// The uncommitted diff, for harnesses that do not report what their write
/// touched. The scan covers the whole tree, so it runs only if a job needs it.
struct GitFallback {
    root: Option<PathBuf>,
    changed: OnceCell<Option<HashMap<PathBuf, Lines>>>,
}

impl GitFallback {
    fn discover() -> GitFallback {
        GitFallback {
            root: git::root().ok().map(|r| r.canonicalize().unwrap_or(r)),
            changed: OnceCell::new(),
        }
    }

    fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// `None` only when git positively reports the file as unchanged — there is
    /// no basis to strip it. When git cannot tell us anything at all (no
    /// repository, or the diff failed) the whole file is the agent's as far as
    /// we can know.
    fn lines_for(&self, path: &Path) -> Option<Lines> {
        let changed = self
            .root
            .as_deref()
            .and_then(|root| self.changed.get_or_init(|| Self::scan(root)).as_ref());
        match changed {
            None => Some(Lines::All),
            Some(changed) => changed.get(path).cloned(),
        }
    }

    fn scan(root: &Path) -> Option<HashMap<PathBuf, Lines>> {
        let changes = git::changes(git::Scope::All)
            .inspect_err(|e| eprintln!("silence: git scan failed, stripping whole files: {e}"))
            .ok()?;
        Some(
            changes
                .files
                .into_iter()
                .map(|(rel, lines)| {
                    let abs = root.join(rel);
                    (abs.canonicalize().unwrap_or(abs), lines)
                })
                .collect(),
        )
    }
}

/// A harness can name the same file twice — relative in the tool input,
/// absolute in the response. Canonical paths collapse those into one job, and
/// the copy carrying a patch wins over the one that would fall back to git;
/// otherwise the fallback would strip the whole uncommitted diff behind it.
fn dedupe_by_canonical_path(jobs: &mut Vec<HookJob>) {
    for job in jobs.iter_mut() {
        if let Ok(canon) = job.path.canonicalize() {
            job.path = canon;
        }
    }
    jobs.sort_by(|a, b| a.path.cmp(&b.path));
    jobs.dedup_by(|dropped, kept| {
        dropped.path == kept.path && {
            if kept.lines.is_none() {
                kept.lines = dropped.lines.take();
            }
            true
        }
    });
}

/// Only the git fallback needs a repository, so the containment check applies to
/// jobs that will use it. A job that already knows which lines the write added
/// does not care where the file lives.
fn hook_targets(mut jobs: Vec<HookJob>, root: Option<&Path>) -> Vec<HookJob> {
    dedupe_by_canonical_path(&mut jobs);
    let mut kept = Vec::with_capacity(jobs.len());
    for job in jobs {
        if !job.path.is_file() {
            log_skip(HookSkip::NotAFile(job.path));
            continue;
        }
        if job.lines.is_none() && root.is_some_and(|root| !job.path.starts_with(root)) {
            log_skip(HookSkip::OutsideRepo(job.path));
            continue;
        }
        if lang_for(&job.path).is_none() {
            log_skip(HookSkip::UnsupportedLang(job.path));
            continue;
        }
        kept.push(job);
    }
    kept
}

pub fn run_hook(explicit: &[PathBuf], preserve: &PreserveConfig) {
    let fallback = GitFallback::discover();
    let jobs = if explicit.is_empty() {
        read_stdin_jobs()
    } else {
        explicit
            .iter()
            .map(|path| HookJob {
                path: path.clone(),
                lines: None,
            })
            .collect()
    };
    let targets = hook_targets(jobs, fallback.root());
    if targets.is_empty() {
        return;
    }

    let opts = |lines| StripOpts {
        line_mode: LineMode::Collapse,
        preserve: preserve.clone(),
        lines,
        kinds: CommentKinds::default(),
        write: WriteMode::Hook,
    };

    let mut stripped: Vec<(PathBuf, usize)> = Vec::new();
    for HookJob { path, lines } in targets {
        let Some(lines) = lines.or_else(|| fallback.lines_for(&path)) else {
            log_skip(HookSkip::NotInGitChanges(path));
            continue;
        };
        match strip_file(&path, &opts(lines)) {
            StripOutcome::Hook { removed } => stripped.push((path, removed)),
            StripOutcome::Unchanged | StripOutcome::Checked { .. } | StripOutcome::Wrote { .. } => {
            }
            StripOutcome::Failed { msg } => log_skip(HookSkip::StripFailed(path, msg)),
            StripOutcome::NoLang => log_skip(HookSkip::UnsupportedLang(path)),
        }
    }

    report_stripped(&stripped);
}

/// Per-file note on stderr (the agent's debug/transcript view), plus a stdout
/// JSON payload carrying a model-facing note. Claude Code and Codex consume the
/// `hookSpecificOutput.additionalContext` field natively; the Opencode and Pi
/// plugins capture this stdout and splice the note into the tool result so the
/// model learns the comments were stripped and stops re-adding them.
fn report_stripped(stripped: &[(PathBuf, usize)]) {
    if stripped.is_empty() {
        return;
    }
    let mut total = 0;
    for (path, removed) in stripped {
        total += removed;
        eprintln!(
            "silence: stripped {removed} comment(s) from {}",
            path.display()
        );
    }
    println!("{}", context_payload(total));
}

fn context_payload(total: usize) -> serde_json::Value {
    let noun = if total == 1 { "comment" } else { "comments" };
    let context =
        format!("silence stripped {total} {noun}. Don't re-add. Prefer self-explanatory code.");
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": context,
        }
    })
}

fn log_skip(skip: HookSkip) {
    match skip {
        HookSkip::NotAFile(path) => {
            eprintln!("silence: skip {}: not a file", path.display());
        }
        HookSkip::NotInGitChanges(path) => {
            eprintln!(
                "silence: skip {}: not in uncommitted changes",
                path.display()
            );
        }
        HookSkip::UnsupportedLang(path) => {
            eprintln!("silence: skip {}: unsupported language", path.display());
        }
        HookSkip::StripFailed(path, msg) => {
            eprintln!("silence: skip {}: {msg}", path.display());
        }
        HookSkip::OutsideRepo(path) => {
            eprintln!("silence: skip {}: outside repository root", path.display());
        }
    }
}

fn read_stdin_jobs() -> Vec<HookJob> {
    match read_stdin().and_then(|input| hook_input::jobs_from_stdin(&input)) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("silence: skip hook stdin: {e}");
            Vec::new()
        }
    }
}

fn read_stdin() -> Result<String, String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_carries_event_name_and_context() -> Result<(), Box<dyn std::error::Error>> {
        let payload = context_payload(3);
        let out = &payload["hookSpecificOutput"];
        assert_eq!(out["hookEventName"], "PostToolUse");
        let ctx = out["additionalContext"]
            .as_str()
            .ok_or("additionalContext is not a string")?;
        assert!(ctx.contains("3 comments"), "total count: {ctx}");
        assert!(ctx.contains("re-add"), "guidance: {ctx}");
        Ok(())
    }

    #[test]
    fn payload_uses_singular_for_one_comment() -> Result<(), Box<dyn std::error::Error>> {
        let payload = context_payload(1);
        let ctx = payload["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .ok_or("additionalContext is not a string")?;
        assert!(ctx.contains("1 comment."), "singular noun: {ctx}");
        Ok(())
    }
}
