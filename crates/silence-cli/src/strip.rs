use anyhow::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use silence_core::{strip, CommentKinds, LineMode, Options, PreserveConfig};
use silence_langs::Lang;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::git;
use silence_grammars::paths::home_dir;

pub type LineRange = (usize, usize);
pub type LineRanges = Vec<LineRange>;
pub type StripJob = (PathBuf, LineRanges);

pub fn lang_for(path: &Path) -> Option<Lang> {
    let ext = path.extension()?.to_str()?;
    Lang::from_extension(ext)
}

pub fn comment_kinds_from_flags(inline: bool, block: bool) -> CommentKinds {
    match (inline, block) {
        (true, false) => CommentKinds {
            line: true,
            block: false,
        },
        (false, true) => CommentKinds {
            line: false,
            block: true,
        },
        _ => CommentKinds::default(),
    }
}

pub struct BatchSettings {
    pub line_mode: LineMode,
    pub preserve: PreserveConfig,
    pub kinds: CommentKinds,
    pub check: bool,
    pub backup: bool,
    pub verbose: bool,
}

pub struct BatchOutcome {
    pub removed_total: usize,
    pub files_with_comments: usize,
    pub error_count: usize,
}

#[derive(Clone)]
pub enum WriteMode {
    Check,
    Write { backup: bool, verbose: bool },
    Hook,
}

#[derive(Clone)]
pub struct StripOpts {
    pub line_mode: LineMode,
    pub preserve: PreserveConfig,
    pub line_ranges: LineRanges,
    pub kinds: CommentKinds,
    pub write: WriteMode,
}

pub enum StripOutcome {
    NoLang,
    Unchanged,
    Checked { removed: usize },
    Wrote { removed: usize },
    Hook { removed: usize },
    Failed { msg: String },
}

pub fn collect_paths(root: &Path) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut builder = WalkBuilder::new(root);
    builder.standard_filters(true);
    builder.require_git(false);
    builder.add_custom_ignore_filename(".silenceignore");
    let global = home_dir().join(".config/.silenceignore");
    if global.is_file() {
        let _ = builder.add_ignore(global);
    }
    let mut out = Vec::new();
    for entry in builder.build() {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() && lang_for(p).is_some() {
            out.push(p.to_path_buf());
        }
    }
    Ok(out)
}

pub fn build_jobs(paths: &[PathBuf], git_scope: Option<git::Scope>) -> Result<Vec<StripJob>> {
    if let Some(scope) = git_scope {
        let ch = git::changes(scope)?;
        return Ok(ch
            .files
            .into_iter()
            .map(|(rel, ranges)| (ch.root.join(rel), ranges))
            .filter(|(p, _)| lang_for(p).is_some())
            .collect());
    }
    if paths.is_empty() {
        anyhow::bail!("at least one path is required unless using --staged, --unstaged, or --changes on `silence strip`");
    }
    let mut out = Vec::new();
    for p in paths {
        for f in collect_paths(p)? {
            out.push((f, Vec::new()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    Ok(out)
}

pub fn strip_file(path: &Path, opts: &StripOpts) -> StripOutcome {
    let Some(lang) = lang_for(path) else {
        return StripOutcome::NoLang;
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return StripOutcome::Failed { msg: e.to_string() };
        }
    };

    let core_opts = Options {
        line_mode: opts.line_mode,
        preserve: opts.preserve.clone(),
        line_ranges: opts.line_ranges.clone(),
        kinds: opts.kinds,
    };

    let outcome = match strip(&source, lang, &core_opts) {
        Ok(o) => o,
        Err(e) => {
            return StripOutcome::Failed { msg: e.to_string() };
        }
    };

    if outcome.removed == 0 {
        return StripOutcome::Unchanged;
    }

    match opts.write {
        WriteMode::Check => {
            println!("{}: {} comment(s)", path.display(), outcome.removed);
            StripOutcome::Checked {
                removed: outcome.removed,
            }
        }
        WriteMode::Hook => {
            if outcome.output == source {
                return StripOutcome::Unchanged;
            }
            if std::fs::write(path, &outcome.output).is_err() {
                return StripOutcome::Failed {
                    msg: "write failed".into(),
                };
            }
            StripOutcome::Hook {
                removed: outcome.removed,
            }
        }
        WriteMode::Write { backup, verbose } => {
            if outcome.output == source {
                return StripOutcome::Unchanged;
            }
            if backup {
                if let Err(e) = std::fs::copy(path, backup_path(path)) {
                    return StripOutcome::Failed {
                        msg: format!("backup failed: {e}"),
                    };
                }
            }
            if let Err(e) = std::fs::write(path, &outcome.output) {
                return StripOutcome::Failed { msg: e.to_string() };
            }
            if verbose {
                eprintln!("  {} (-{} comments)", path.display(), outcome.removed);
            }
            StripOutcome::Wrote {
                removed: outcome.removed,
            }
        }
    }
}

pub fn run_batch(
    jobs: &[StripJob],
    settings: &BatchSettings,
    restage: bool,
) -> Result<BatchOutcome> {
    let results: Vec<StripOutcome> = jobs
        .par_iter()
        .map(|(path, ranges)| {
            strip_file(
                path,
                &StripOpts {
                    line_mode: settings.line_mode,
                    preserve: settings.preserve.clone(),
                    line_ranges: ranges.clone(),
                    kinds: settings.kinds,
                    write: if settings.check {
                        WriteMode::Check
                    } else {
                        WriteMode::Write {
                            backup: settings.backup,
                            verbose: settings.verbose,
                        }
                    },
                },
            )
        })
        .collect();

    let removed_total = AtomicUsize::new(0);
    let files_with_comments = AtomicUsize::new(0);
    let mut error_count = 0usize;
    let mut changed_paths = Vec::new();

    for ((path, _), outcome) in jobs.iter().zip(results) {
        match outcome {
            StripOutcome::Wrote { removed } => {
                removed_total.fetch_add(removed, Ordering::Relaxed);
                files_with_comments.fetch_add(1, Ordering::Relaxed);
                changed_paths.push(path.clone());
            }
            StripOutcome::Checked { removed } if removed > 0 => {
                removed_total.fetch_add(removed, Ordering::Relaxed);
                files_with_comments.fetch_add(1, Ordering::Relaxed);
            }
            StripOutcome::Failed { msg } => {
                error_count += 1;
                eprintln!("  skip {}: {msg}", path.display());
            }
            StripOutcome::NoLang
            | StripOutcome::Unchanged
            | StripOutcome::Hook { .. }
            | StripOutcome::Checked { .. } => {}
        }
    }

    if restage && !settings.check {
        git::stage_paths(&changed_paths)?;
    }

    Ok(BatchOutcome {
        removed_total: removed_total.load(Ordering::Relaxed),
        files_with_comments: files_with_comments.load(Ordering::Relaxed),
        error_count,
    })
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}
