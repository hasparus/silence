<img alt="silence meme, masked King Baldwin from Kingdom of Heaven raising his hand" src="./king-baldwin-iv-kingdom-of-heaven.png" width=256 />

# silence

Removes (or warns about) comments in an agent post-write hook. Preserves
doc/directive comments and `TODO`/`FIXME`/`HACK` by default. Tree-sitter
based, so it doesn't get confused by `//` inside strings. Works with Claude
Code, Codex, Opencode, and Pi.

## install

yolo curl to shell

```sh
curl -sSf https://raw.githubusercontent.com/hasparus/silence/main/install.sh | sh
```

install with cargo

```sh
cargo install silence-cli
```

or grab a binary from [Releases](https://github.com/hasparus/silence/releases).

## tldr

```
silence --install-hook         # wire silence into your AI agent's post-edit hook

silence src/ -r                # strip a tree (respects .gitignore and .silenceignore)
silence file.rs --check        # exit 1 if comments present, don't write files
silence --staged               # only strip comments inside staged hunks
silence --changes              # strip comments inside all uncommitted changes
silence file.py --preserve-lines   # keep blank lines where comments were

silence --llm # print a short guide for llms
```

## rant

Yes, machine, you have obeyed. Yes, the code is blazing fast, just like I
asked. Yes it's "no-slop and production-grade". Neither the code reviewer,
nor future me, nor future you spending tokens reading this code needs to
know that I asked.

## usage

### flags

**processing**

- `<path>` — file or directory (omit when using a git mode or action flag)
- `-r, --recursive` — recurse into subdirectories
- `--check` — print what would be removed; exit 1 if any.
- `--inline` — remove only line comments (`//`, `#`)
- `--block` — remove only block comments (`/* … */`)
- `--preserve-lines` — leave blank lines where comments were, instead of collapsing
- `--backup` — write a `<file>.bak` next to each modified file
- `--no-default-preserve` — drop the built-in preserve list and directive detection
- `--threads N` — parallelism (default: CPU count)
- `--verbose` — verbose output

**git scoping**

- `--staged` — only comments inside staged hunks
- `--unstaged` — only comments inside unstaged + untracked changes
- `--changes` (alias `--changes-only`) — strip comments inside all uncommitted changes

**agent hooks**

- `--install-hook` — wire `silence --hook` into `~/.claude/`, `~/.codex/`,
  `~/.config/opencode/plugins/`, `~/.pi/agent/extensions/`
- `--install-hook --to codex --to claude` — install only selected agent hooks
- `--install-hook --project` — same files under the current directory
- `--uninstall-hook` — clean up installed hooks
- `--hook-status` (alias `--list-hooks`) — per-agent install state
- `--project` — scope the hook commands to the current project instead of `~`
- `--hook` — reads a path arg or the agent's stdin event, strips comments inside the uncommitted change, always exits 0

**config**

- `--config` — print the active configuration and where it came from
- `--create-config` — write an example `.silence.toml` to the current directory
- `--llm` — print a short usage guide written for AI agents

## preserve rules

A comment is kept when it matches a preserve pattern **or looks like a
directive**: i.e. body starting with `@`, shaped `namespace:value`, or an XML-ish
`<tag … />` (so `@ts-ignore`, `//go:embed`, `/// <reference />` survive
without a rule each). A `#!` shebang on line 1 is never removed.

`.silence.toml` (searched cwd → git root → `~/.config/.silence.toml`):

```toml
preserve = ["TODO", "FIXME", "*IMPORTANT*"]   # extra patterns; globs allowed
# use_default_preserve = false                # drop built-ins (default: true)
```

`--no-default-preserve` drops the built-ins and directive detection but keeps
your `preserve` list. `.silenceignore` (same format as `.gitignore`, optionally
at `~/.config/.silenceignore`) excludes files from walks.

## contributing

### design

Tree-sitter parses each file to a CST; a tiny `(comment) @comment` query
returns exact byte spans; spans matching preserve patterns are dropped (and,
in git mode, spans outside changed line ranges); the file is reassembled.
The engine ([`silence-core`](crates/silence-core)) is I/O-free so the logic is
unit-tested in isolation and u can build on top of it.

### build

Needs a Rust toolchain (1.82+). `git2` links libgit2 (vendored by default
via the crate; system `cmake`/C toolchain may be required on first build).

```
cargo build --release
cargo test
./target/release/silence --help
```

### adding a language

1. add a grammar crate to the workspace `Cargo.toml`
2. update `Lang::from_extension`, `grammar()`, and `comment_query()`.
3. `every_grammar_loads_and_query_compiles` test verifies the grammar/query/ABI
   line up.
