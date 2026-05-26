use silence_core::{CommentKinds, LineMode, PreserveConfig};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::git;
use crate::hook_input;
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

fn hook_targets(explicit: &[PathBuf], state: Option<&GitState>) -> Vec<PathBuf> {
    let mut targets = if explicit.is_empty() {
        match hook_targets_from_stdin() {
            Ok(paths) => paths,
            Err(e) => {
                eprintln!("silence: skip hook stdin: {e}");
                Vec::new()
            }
        }
    } else {
        explicit.to_vec()
    };
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
    let targets = hook_targets(explicit, state.as_ref());
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
            StripOutcome::Hook
            | StripOutcome::Unchanged
            | StripOutcome::Checked { .. }
            | StripOutcome::Wrote { .. } => {}
            StripOutcome::Failed { msg } => log_skip(HookSkip::StripFailed(path, msg)),
            StripOutcome::NoLang => log_skip(HookSkip::UnsupportedLang(path)),
        }
    }
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

fn hook_targets_from_stdin() -> Result<Vec<PathBuf>, String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    hook_input::paths_from_stdin(&input)
}
