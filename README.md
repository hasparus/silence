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

## usage

```
silence src/ -r                # strip a tree (respects .gitignore / .silenceignore)
silence file.rs --check        # report only; exit 1 if comments present
silence --staged               # only comments inside staged hunks
silence --changes              # staged + unstaged + untracked
silence file.py --preserve-lines   # keep blank lines where comments were
silence --install-hook         # wire silence into your AI agent's post-edit hook
```

## rant

Yes, machine, you have obeyed. Yes, the code is blazing fast, just like I
asked. Yes it's "no-slop and production-grade". Neither the code reviewer,
nor future me, nor future you spending tokens reading this code needs to
know that I asked.

### flags

**Processing**

- `<path>` — file or directory (omit when using a git mode or action flag)
- `-r, --recursive` — recurse into subdirectories
- `--check` — print what would be removed; exit 1 if any. Suits CI gates.
- `--inline` — remove only line comments (`//`, `#`)
- `--block` — remove only block comments (`/* … */`)
- `--preserve-lines` — leave blank lines where comments were, instead of collapsing
- `--backup` — write a `<file>.bak` next to each modified file
- `--no-default-preserve` — drop the built-in preserve list and directive detection
- `--threads N` — parallelism (default: CPU count)
- `--verbose` — narrate what's happening

**Git scoping**

- `--staged` — only comments inside staged hunks
- `--unstaged` — only comments inside unstaged + untracked changes
- `--changes` (alias `--changes-only`) — staged + unstaged + untracked

**Agent hooks**

- `--install-hook` — wire `silence --hook` into `~/.claude/`, `~/.codex/`,
  `~/.config/opencode/plugins/`, `~/.pi/agent/extensions/`
- `--install-hook --to codex --to claude` — install only selected agent hooks
- `--install-hook --project` — same files under the current directory
- `--uninstall-hook` — clean up installed hooks
- `--hook-status` (alias `--list-hooks`) — per-agent install state
- `--project` — scope the hook commands to the current project instead of `~`
- `--hook` — the post-edit handler itself: reads a path arg or the agent's
  stdin event, strips comments inside the uncommitted change, always exits 0

**Config**

- `--config` — print the active configuration and where it came from
- `--create-config` — write an example `.silence.toml` to the current directory
- `--llm` — print a short usage guide written for AI agents

## preserve rules

A comment is kept when it matches a preserve pattern **or looks like a
directive** — body starting with `@`, shaped `namespace:value`, or an XML-ish
`<tag … />` (so `@ts-ignore`, `//go:embed`, `/// <reference />` survive
without a rule each). A `#!` shebang on line 1 is never removed.

**Machine markers** are kept too: up to three identifier-shaped tokens, each
carrying a separator or a digit, so the paired sentinels other tools write
between (`{/* impeccable-variants-start cd383158 */}`, `// prettier-ignore-start`)
survive. One ordinary word disqualifies the comment, so `// well-known trick
here` still goes.

`.silence.toml` (searched cwd → git root → `~/.config/.silence.toml`):

```toml
preserve = ["TODO", "FIXME", "*IMPORTANT*"]   # extra patterns; globs allowed
# use_default_preserve = false                # drop built-ins (default: true)
```

`--no-default-preserve` drops the built-ins and directive detection but keeps
your `preserve` list. `.silenceignore` (same format as `.gitignore`, optionally
at `~/.config/.silenceignore`) excludes files from walks.

## design

Tree-sitter parses each file to a CST; a tiny `(comment) @comment` query
returns exact byte spans; spans matching preserve patterns are dropped (and,
in git mode, spans outside changed line ranges); the file is reassembled.
A `/* … */` spanning five lines is one node with one span — no line-by-line
state to desync, no `// inside "a string"` false positives. The engine
([`silence-core`](crates/silence-core)) is I/O-free so the tricky logic is
unit-tested in isolation and u can build on top of it.

## build

Needs a Rust toolchain (1.82+). `git2` links libgit2 (vendored by default
via the crate; system `cmake`/C toolchain may be required on first build).

```
cargo build --release
cargo test
./target/release/silence --help
```

## adding a language

1. Add the grammar crate to the workspace `Cargo.toml`.
2. Add an arm to `Lang::from_extension`, `grammar()`, and `comment_query()`.
3. Done — the engine is untouched. The
   `every_grammar_loads_and_query_compiles` test verifies the grammar/query/ABI
   line up.
