use anyhow::{Context, Result};
use git2::{Diff, DiffOptions, Repository};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    Staged,
    Unstaged,
    All,
}

pub struct GitChanges {
    pub root: PathBuf,
    pub files: HashMap<PathBuf, Vec<(usize, usize)>>,
}

pub fn changes(scope: Scope) -> Result<GitChanges> {
    let repo = Repository::discover(".").context("not inside a git repository")?;
    let root = repo
        .workdir()
        .context("bare repositories are not supported")?
        .to_path_buf();

    let mut files: HashMap<PathBuf, Vec<(usize, usize)>> = HashMap::new();

    match scope {
        Scope::Staged => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let staged = repo
                .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut diff_opts()))
                .context("failed to diff staged changes")?;
            let unstaged = repo
                .diff_index_to_workdir(None, Some(&mut diff_opts()))
                .context("failed to diff unstaged changes")?;
            let dirty = diff_paths(&unstaged);
            collect_hunks(&staged, &mut files)?;
            files.retain(|path, _| {
                let keep = !dirty.contains(path);
                if !keep {
                    eprintln!(
                        "  skip {}: has unstaged changes; staged line ranges cannot be mapped safely",
                        path.display()
                    );
                }
                keep
            });
        }
        Scope::Unstaged => {
            let diff = repo
                .diff_index_to_workdir(None, Some(&mut diff_opts()))
                .context("failed to diff unstaged changes")?;
            collect_hunks(&diff, &mut files)?;
            collect_untracked(&repo, &mut files)?;
        }
        Scope::All => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let staged = repo
                .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut diff_opts()))
                .context("failed to diff staged changes")?;
            let unstaged = repo
                .diff_index_to_workdir(None, Some(&mut diff_opts()))
                .context("failed to diff unstaged changes")?;
            let conflicted: HashSet<PathBuf> = diff_paths(&staged)
                .intersection(&diff_paths(&unstaged))
                .cloned()
                .collect();
            collect_hunks(&staged, &mut files)?;
            collect_hunks(&unstaged, &mut files)?;
            collect_untracked(&repo, &mut files)?;
            for path in conflicted {
                if let Some(ranges) = files.get_mut(&path) {
                    ranges.clear();
                }
            }
        }
    }

    Ok(GitChanges { root, files })
}

fn diff_opts() -> DiffOptions {
    let mut o = DiffOptions::new();
    o.context_lines(0);
    o
}

fn diff_paths(diff: &Diff) -> HashSet<PathBuf> {
    diff.deltas()
        .filter_map(|d| d.new_file().path().map(Path::to_path_buf))
        .collect()
}

fn collect_hunks(diff: &Diff, files: &mut HashMap<PathBuf, Vec<(usize, usize)>>) -> Result<()> {
    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        Some(&mut |delta, hunk| {
            if let Some(path) = delta.new_file().path() {
                let start = hunk.new_start() as usize;
                let count = hunk.new_lines() as usize;
                if count > 0 {
                    files
                        .entry(path.to_path_buf())
                        .or_default()
                        .push((start, start + count - 1));
                }
            }
            true
        }),
        None,
    )?;
    Ok(())
}

fn collect_untracked(
    repo: &Repository,
    files: &mut HashMap<PathBuf, Vec<(usize, usize)>>,
) -> Result<()> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    for entry in statuses.iter() {
        if entry.status().is_wt_new() {
            if let Ok(p) = entry.path() {
                files.entry(PathBuf::from(p)).or_default();
            }
        }
    }
    Ok(())
}
