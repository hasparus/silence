use serde_json::json;
use silence_core::{CommentKinds, LineMode, PreserveConfig};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::git;
use crate::hook_input::{self, HookEvent};
use crate::strip::{lang_for, strip_file, LineRanges, StripOpts, StripOutcome, WriteMode};

enum HookSkip {
    NotInGitChanges(PathBuf),
    UnsupportedLang(PathBuf),
    StripFailed(PathBuf, String),
    OutsideRepo(PathBuf),
}

struct GitState {
    root: PathBuf,
    ranges: HashMap<PathBuf, LineRanges>,
}

fn git_state() -> Option<GitState> {
    let ch = git::changes(git::Scope::All).ok()?;
    let root = ch.root.canonicalize().unwrap_or(ch.root);
    let mut ranges = HashMap::new();
    for (rel, r) in ch.files {
        let abs = root.join(rel);
        let key = abs.canonicalize().unwrap_or(abs);
        ranges.insert(key, r);
    }
    Some(GitState { root, ranges })
}

fn hook_targets(mut targets: Vec<PathBuf>, state: Option<&GitState>) -> Vec<PathBuf> {
    targets.sort();
    targets.dedup();
    let mut kept = Vec::with_capacity(targets.len());
    for path in targets {
        if !path.is_file() {
            eprintln!("silence: skip {}: not a file", path.display());
            continue;
        }
        if let Some(s) = state {
            match path.canonicalize() {
                Ok(canon) if canon.starts_with(&s.root) => {}
                _ => {
                    log_skip(HookSkip::OutsideRepo(path));
                    continue;
                }
            }
        }
        if lang_for(&path).is_none() {
            log_skip(HookSkip::UnsupportedLang(path));
            continue;
        }
        kept.push(path);
    }
    kept
}

pub fn run_hook(explicit: &[PathBuf], preserve: &PreserveConfig) {
    let state = git_state();
    let event = if explicit.is_empty() {
        read_stdin_event()
    } else {
        HookEvent {
            paths: explicit.to_vec(),
            claude_event: None,
        }
    };
    let targets = hook_targets(event.paths.clone(), state.as_ref());
    if targets.is_empty() {
        return;
    }

    let opts = StripOpts {
        line_mode: LineMode::Collapse,
        preserve: preserve.clone(),
        line_ranges: Vec::new(),
        kinds: CommentKinds::default(),
        write: WriteMode::Hook,
    };

    let mut stripped: Vec<(PathBuf, usize)> = Vec::new();
    for path in targets {
        let Some(ranges) = hook_line_ranges(&path, state.as_ref()) else {
            log_skip(HookSkip::NotInGitChanges(path));
            continue;
        };
        let file_opts = StripOpts {
            line_ranges: ranges,
            ..opts.clone()
        };
        match strip_file(&path, &file_opts) {
            StripOutcome::Hook { removed } => stripped.push((path, removed)),
            StripOutcome::Unchanged | StripOutcome::Checked { .. } | StripOutcome::Wrote { .. } => {
            }
            StripOutcome::Failed { msg } => log_skip(HookSkip::StripFailed(path, msg)),
            StripOutcome::NoLang => log_skip(HookSkip::UnsupportedLang(path)),
        }
    }

    report_stripped(&stripped, &event);
}

/// Per-file note on stderr (visible in the agent's debug/transcript view), plus
/// — for Claude Code — a stdout JSON payload that feeds the model context so it
/// stops re-adding the comments silence just removed.
fn report_stripped(stripped: &[(PathBuf, usize)], event: &HookEvent) {
    if stripped.is_empty() {
        return;
    }
    for (path, removed) in stripped {
        eprintln!(
            "silence: stripped {removed} comment(s) from {}",
            path.display()
        );
    }
    if let Some(name) = event.claude_event.as_deref() {
        emit_claude_context(name, stripped);
    }
}

fn emit_claude_context(event_name: &str, stripped: &[(PathBuf, usize)]) {
    println!("{}", claude_context_payload(event_name, stripped));
}

fn claude_context_payload(event_name: &str, stripped: &[(PathBuf, usize)]) -> serde_json::Value {
    let total: usize = stripped.iter().map(|(_, removed)| removed).sum();
    let files = stripped
        .iter()
        .map(|(path, _)| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let context = format!(
        "silence removed {total} comment(s) it judged redundant from {files}. \
         This project strips comments that restate or narrate the code. Do not \
         re-add them: prefer self-explanatory code, and keep only public-API docs, \
         the reasoning behind non-obvious choices, and directive comments \
         (e.g. eslint-disable, @ts-expect-error, noqa)."
    );
    json!({
        "hookSpecificOutput": {
            "hookEventName": event_name,
            "additionalContext": context,
        }
    })
}

fn log_skip(skip: HookSkip) {
    match skip {
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

fn hook_line_ranges(path: &Path, state: Option<&GitState>) -> Option<LineRanges> {
    let Some(s) = state else {
        return Some(Vec::new());
    };
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    s.ranges.get(&canon).cloned()
}

fn read_stdin_event() -> HookEvent {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("silence: skip hook stdin: {e}");
        return HookEvent {
            paths: Vec::new(),
            claude_event: None,
        };
    }
    match hook_input::event_from_stdin(&input) {
        Ok(event) => event,
        Err(e) => {
            eprintln!("silence: skip hook stdin: {e}");
            HookEvent {
                paths: Vec::new(),
                claude_event: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_payload_carries_event_name_and_context() {
        let stripped = vec![
            (PathBuf::from("src/a.ts"), 2),
            (PathBuf::from("src/b.ts"), 1),
        ];
        let payload = claude_context_payload("PostToolUse", &stripped);
        let out = &payload["hookSpecificOutput"];
        assert_eq!(out["hookEventName"], "PostToolUse");
        let ctx = out["additionalContext"].as_str().unwrap();
        assert!(ctx.contains("3 comment(s)"), "total count: {ctx}");
        assert!(ctx.contains("src/a.ts, src/b.ts"), "file list: {ctx}");
        assert!(ctx.contains("Do not"), "guidance: {ctx}");
    }
}
