use serde_json::json;
use silence_core::{CommentKinds, LineMode, Lines, PreserveConfig};
use std::cell::RefCell;
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

/// Uncommitted diffs for harnesses that do not report what their write touched.
/// Hooks may run from an agent's config directory rather than the edited repo,
/// so each target discovers its own worktree. Scans stay lazy and are shared by
/// targets in the same worktree.
struct GitFallback {
    process_root: Option<PathBuf>,
    changed_by_root: RefCell<HashMap<PathBuf, Option<HashMap<PathBuf, Lines>>>>,
}

impl GitFallback {
    fn discover() -> GitFallback {
        GitFallback {
            process_root: git::root()
                .ok()
                .map(|root| root.canonicalize().unwrap_or(root)),
            changed_by_root: RefCell::new(HashMap::new()),
        }
    }

    /// One answer per path, including which skip applies when there is none.
    ///
    /// When git cannot tell us anything at all (no repository, or the diff
    /// failed) the whole file is the agent's as far as we can know. Only a file
    /// git positively reports as unchanged gives no basis to strip.
    fn lines_for(&self, path: &Path) -> Result<Lines, HookSkip> {
        let root = match git::root_from(path) {
            Ok(root) => root.canonicalize().unwrap_or(root),
            Err(_) if self.process_root.is_some() => {
                return Err(HookSkip::OutsideRepo(path.to_path_buf()));
            }
            Err(_) => return Ok(Lines::All),
        };

        let mut scans = self.changed_by_root.borrow_mut();
        let changed = scans
            .entry(root.clone())
            .or_insert_with(|| Self::scan(&root));
        match changed {
            None => Ok(Lines::All),
            Some(changed) => changed
                .get(path)
                .cloned()
                .ok_or_else(|| HookSkip::NotInGitChanges(path.to_path_buf())),
        }
    }

    fn scan(root: &Path) -> Option<HashMap<PathBuf, Lines>> {
        let changes = git::changes_from(root, git::Scope::All)
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
///
/// An event describes at most one patch, so at most one job ever carries lines
/// and there is never a choice between two of them.
fn dedupe_by_canonical_path(jobs: &mut Vec<HookJob>) {
    for job in jobs.iter_mut() {
        if let Ok(canon) = job.path.canonicalize() {
            job.path = canon;
        }
    }
    jobs.sort_by(|a, b| a.path.cmp(&b.path));
    jobs.dedup_by(|dropped, kept| {
        if dropped.path != kept.path {
            return false;
        }
        kept.lines = kept.lines.take().or_else(|| dropped.lines.take());
        true
    });
}

/// Settles every question about a job in one place: which file it really names,
/// whether we can strip it at all, and which of its lines are in scope. Asking
/// git happens here too, so "does this job need a repository" is decided once,
/// where the answer is used, rather than predicted by an earlier pass. Every
/// reason to skip comes back as a value rather than a branch out of the loop.
fn resolve(mut jobs: Vec<HookJob>, fallback: &GitFallback) -> Vec<(PathBuf, Lines)> {
    dedupe_by_canonical_path(&mut jobs);
    jobs.into_iter()
        .filter_map(|job| match resolve_job(job, fallback) {
            Ok(resolved) => Some(resolved),
            Err(skip) => {
                log_skip(skip);
                None
            }
        })
        .collect()
}

fn resolve_job(
    HookJob { path, lines }: HookJob,
    fallback: &GitFallback,
) -> Result<(PathBuf, Lines), HookSkip> {
    if !path.is_file() {
        return Err(HookSkip::NotAFile(path));
    }
    if lang_for(&path).is_none() {
        return Err(HookSkip::UnsupportedLang(path));
    }
    let lines = match lines {
        Some(lines) => lines,
        None => fallback.lines_for(&path)?,
    };
    Ok((path, lines))
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
    let opts = |lines| StripOpts {
        line_mode: LineMode::Collapse,
        preserve: preserve.clone(),
        lines,
        kinds: CommentKinds::default(),
        write: WriteMode::Hook,
    };

    let mut stripped: Vec<(PathBuf, usize)> = Vec::new();
    for (path, lines) in resolve(jobs, &fallback) {
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
        Ok(jobs) => jobs,
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
