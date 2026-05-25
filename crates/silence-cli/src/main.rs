mod config;
mod git;
mod hook_input;
mod hook_run;
mod hooks;
mod paths;
mod strip;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::{create_example_config, LoadedConfig};
use silence_core::{LineMode, PreserveConfig};
use std::path::PathBuf;

use strip::{build_jobs, comment_kinds_from_flags, run_batch, BatchSettings};

#[derive(Parser, Debug)]
#[command(
    name = "silence",
    version,
    about = "Remove or check for comments in source code, using tree-sitter."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Strip(StripArgs),
    Hook(HookArgs),
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Llm,
}

#[derive(Subcommand, Debug)]
enum HooksCommand {
    Install(HooksArgs),
    Uninstall(HooksArgs),
    Status(HooksArgs),
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    Show(ShowConfigArgs),
    Init,
}

#[derive(clap::Args, Debug)]
struct StripArgs {
    #[arg(conflicts_with_all = ["staged", "unstaged", "changes"])]
    paths: Vec<PathBuf>,

    #[arg(long)]
    check: bool,

    #[arg(short, long)]
    recursive: bool,

    #[arg(long)]
    preserve_lines: bool,

    #[arg(long)]
    no_default_preserve: bool,

    #[arg(long)]
    inline: bool,

    #[arg(long)]
    block: bool,

    #[arg(long)]
    backup: bool,

    #[arg(long, conflicts_with_all = ["unstaged", "changes"])]
    staged: bool,

    #[arg(long, conflicts_with_all = ["staged", "changes"])]
    unstaged: bool,

    #[arg(long, visible_alias = "changes-only", conflicts_with_all = ["staged", "unstaged"])]
    changes: bool,

    #[arg(long)]
    threads: Option<usize>,

    #[arg(long)]
    verbose: bool,
}

#[derive(clap::Args, Debug)]
struct HookArgs {
    paths: Vec<PathBuf>,

    #[arg(long)]
    no_default_preserve: bool,
}

#[derive(clap::Args, Debug)]
struct HooksArgs {
    #[arg(long)]
    project: bool,

    #[arg(long = "to", value_enum)]
    agents: Vec<hooks::Agent>,
}

#[derive(clap::Args, Debug)]
struct ShowConfigArgs {
    #[arg(long)]
    no_default_preserve: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::Llm => {
            print!("{LLM_GUIDE}");
            Ok(())
        }
        Commands::Config { command } => match command {
            ConfigCommand::Init => create_example_config(),
            ConfigCommand::Show(args) => {
                let loaded = LoadedConfig::discover(&std::env::current_dir()?);
                loaded.print_active(args.no_default_preserve);
                Ok(())
            }
        },
        Commands::Hooks { command } => {
            let scope = hooks_scope(&command);
            let agents = hooks_agents(&command);
            match command {
                HooksCommand::Install(_) => hooks::install(scope, agents),
                HooksCommand::Uninstall(_) => hooks::uninstall(scope, agents),
                HooksCommand::Status(_) => hooks::status(scope, agents),
            }
            Ok(())
        }
        Commands::Hook(args) => {
            let preserve = load_preserve(&LoadedConfig::discover(&std::env::current_dir()?), args.no_default_preserve);
            hook_run::run_hook(&args.paths, &preserve);
            Ok(())
        }
        Commands::Strip(args) => {
            if let Some(n) = args.threads {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(n)
                    .build_global()
                    .context("failed to configure thread pool")?;
            }
            let loaded = LoadedConfig::discover(&std::env::current_dir()?);
            run_strip(&args, &loaded, load_preserve(&loaded, args.no_default_preserve))
        }
    }
}

fn hooks_scope(command: &HooksCommand) -> hooks::Scope {
    let project = match command {
        HooksCommand::Install(a) | HooksCommand::Uninstall(a) | HooksCommand::Status(a) => a.project,
    };
    if project {
        hooks::Scope::Project
    } else {
        hooks::Scope::User
    }
}

fn hooks_agents(command: &HooksCommand) -> &[hooks::Agent] {
    match command {
        HooksCommand::Install(a) | HooksCommand::Uninstall(a) | HooksCommand::Status(a) => &a.agents,
    }
}

fn load_preserve(loaded: &LoadedConfig, no_default_preserve: bool) -> PreserveConfig {
    let preserve = loaded.preserve(no_default_preserve);
    for bad in preserve.invalid_patterns() {
        eprintln!("warning: ignoring invalid preserve pattern: {bad}");
    }
    preserve
}

fn run_strip(args: &StripArgs, loaded: &LoadedConfig, preserve: PreserveConfig) -> Result<()> {
    if args.verbose {
        match &loaded.path {
            Some(p) => eprintln!("config: {}", p.display()),
            None => eprintln!("config: built-in defaults"),
        }
    }

    let git_scope = if args.staged {
        Some(git::Scope::Staged)
    } else if args.unstaged {
        Some(git::Scope::Unstaged)
    } else if args.changes {
        Some(git::Scope::All)
    } else {
        None
    };

    let jobs = build_jobs(&args.paths, args.recursive, git_scope)?;
    if jobs.is_empty() {
        eprintln!("no supported files to process");
        return Ok(());
    }

    let outcome = run_batch(
        &jobs,
        &BatchSettings {
            line_mode: if args.preserve_lines {
                LineMode::PreserveLines
            } else {
                LineMode::Collapse
            },
            preserve,
            kinds: comment_kinds_from_flags(args.inline, args.block),
            check: args.check,
            backup: args.backup,
            verbose: args.verbose,
        },
        args.staged,
    )?;

    if args.check {
        if outcome.removed_total > 0 {
            eprintln!(
                "{} comment(s) in {} file(s) would be removed",
                outcome.removed_total, outcome.files_with_comments
            );
        } else if args.verbose {
            eprintln!("no removable comments found");
        }
    } else if args.verbose {
        eprintln!(
            "removed {} comment(s) across {} file(s)",
            outcome.removed_total, outcome.files_with_comments
        );
    }

    if outcome.error_count > 0 {
        eprintln!("{} file(s) could not be processed", outcome.error_count);
        std::process::exit(2);
    }
    if args.check && outcome.removed_total > 0 {
        std::process::exit(1);
    }

    Ok(())
}

const LLM_GUIDE: &str = "\
silence — remove slop comments from source code (tree-sitter based).

USAGE
  silence strip <path>        strip comments from a file or directory
  silence strip <path> -r     recurse into subdirectories
  silence strip <path> --check report only; exit 1 if comments would be removed
  silence strip --staged      strip comments inside staged git hunks
  silence strip --changes     strip comments inside all uncommitted changes
  silence hook [path]         agent post-edit hook (reads stdin when no path)

KEEP RULES
  Ordinary comments are removed. Kept by default: TODO/FIXME/HACK/XXX/SAFETY,
  common lint directives, and directive-shaped comments (@ts-ignore,
  //go:embed, /// <reference/>). Add more in .silence.toml.

LANGUAGES
  Rust, TypeScript, JavaScript, JSX/TSX, Python, Go, TOML.
  Respects .gitignore and .silenceignore.

AGENT GUIDANCE
  Do not write comments that restate the code or narrate the change. To clean
  a file, run `silence strip <file>` rather than deleting comments by hand.
";
