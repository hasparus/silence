<img alt="silence meme, masked King Baldwin from Kingdom of Heaven raising his hand" src="./king-baldwin-iv-kingdom-of-heaven.png" width=256 />

# silence

Strips comments in an agent post-write hook. Preserves doc comments and directives by default (e.g. JSDoc, `eslint-disable-next-line`, `@ts-check`, `noqa:`, `TODO`, `FIXME`, ` HACK`).

Works with **Claude Code**, **Codex**, **Opencode**, and **Pi**. \
Supports TypeScript/JavaScript, Python, Rust, Go, C/C++, Java, Kotlin, C#, Swift, CSS, JSON, YAML, Astro.

Grammars are downloaded lazily to avoid a huge binary.

## install

yolo curl to shell

```sh
curl -sSf https://raw.githubusercontent.com/hasparus/silence/main/install.sh | sh
```

install with cargo

```sh
cargo install silence-cli && silence hook install
```

or grab a binary from [Releases](https://github.com/hasparus/silence/releases).

## tldr

```sh
silence hooks install # wire silence into your AI agent's post-edit hook

silence strip src/ # strip comments in a tree (recursive; respects .gitignore and .silenceignore)
silence strip file1.rs file2.rs --check # exit 1 if comments present, don't write files
silence strip --staged # only strip comments inside staged hunks
silence strip --changes # strip comments inside all uncommitted changes
silence strip file.py --preserve-lines

silence llm # print a short guide for llms
```

## usage

### commands

**`silence strip`** — remove or check for comments

- `<path>…` — file or directory (directories recurse; omit when using a git scope flag)
- `--check` — print what would be removed; exit 1 if any
- `--inline` — remove only line comments (`//`, `#`)
- `--block` — remove only block comments (`/* … */`)
- `--preserve-lines` — leave blank lines where comments were
- `--backup` — write a `<file>.bak` next to each modified file
- `--no-default-preserve` — drop the built-in preserve list and directive detection
- `--threads N` — parallelism (default: CPU count)
- `--verbose` — verbose output
- `--staged` — only comments inside staged hunks
- `--unstaged` — only comments inside unstaged + untracked changes
- `--changes` — strip comments inside all uncommitted changes

**`silence hook`** — agent post-edit hook

- `[path]…` — optional paths; reads the agent's stdin event when omitted
- strips comments inside the uncommitted change; always exits 0
- feeds the model a short note so it learns the comments were stripped and
  stops re-adding them: Claude Code and Codex read the `additionalContext`
  stdout JSON natively; the Opencode and Pi plugins splice it into the tool result
- `--no-default-preserve` — same as on `strip`

**`silence hooks`** — install into agent configs

- `install` — wire `silence hook` into `~/.claude/`, `~/.codex/`,
  `~/.config/opencode/plugins/`, `~/.pi/agent/extensions/`
- `install --to codex --to claude` — selected agents only
- `install --project` — project-local paths under the current dir
- `uninstall` — remove installed hooks
- `status` — per-agent install state

**`silence config`**

- `show` — print active configuration and where it came from
- `show --no-default-preserve` — preview preserve rules with defaults off
- `init` — write an example `.silence.toml` to the current dir

**`silence llm`** — usage guide for agents

## preserve rules

A comment is kept when it matches a preserve pattern **or looks like a
directive**: i.e. body starting with `@`, shaped `namespace:value`, or an XML-ish
`<tag … />` (so `@ts-ignore`, `//go:embed`, `/// <reference />` survive
without a rule each). A `#!` shebang on line 1 is never removed.

**Machine markers** survive too: a sentinel ending in `-start`, `-end` or
`-begin`, optionally followed by up to two identifier-shaped ids, so the paired
sentinels other tools write between (`{/* impeccable-variants-start cd383158 */}`,
`// codegen-end`) are not stripped out from under them. Prose is not shaped like
that, so `// well-known trick here` and `// broken.` still go.

`.silence.toml` (searched cwd → git root → `~/.config/.silence.toml`):

```toml
preserve = ["TODO", "FIXME", "*IMPORTANT*"]   # extra patterns; globs allowed
# use_default_preserve = false                # drop built-ins (default: true)
```

`--no-default-preserve` drops the built-ins and directive detection but keeps
your `preserve` list. `.silenceignore` (same format as `.gitignore`, optionally
at `~/.config/.silenceignore`) excludes files from walks.

## example usage

<img width=576 height=576 alt="silence-gtm" src="https://github.com/user-attachments/assets/5fd06537-d87c-4626-94b7-6dfd49f63735" />

## rant

Yes, machine, you have obeyed. Yes, the code is blazing fast, just like I
asked. Yes it's "no-slop and production-grade". Neither the code reviewer,
nor future me, nor future you spending tokens reading this code needs to
know that I asked.

## contributing

### design

Tree-sitter parses each file to a CST; a tiny `(comment) @comment` query
returns exact byte spans; spans matching preserve patterns are dropped (and,
in git mode, spans outside changed line ranges); the file is reassembled.
The engine ([`silence-core`](crates/silence-core)) is I/O-free so the logic is
unit-tested in isolation and u can build on top of it.

### build

Needs a Rust toolchain (1.85+). `git2` links libgit2 (vendored by default
via the crate; system `cmake`/C toolchain may be required on first build).

```
cargo build --release
cargo test
./target/release/silence --help
```

### hook benchmark

Measures end-to-end `silence hook` latency (process spawn + git scan + parse).
Build release first, then:

```
./scripts/bench-hook.sh
```

Env: `RUNS` (default 50), `WARMUP` (default 5), `SILENCE_BIN` (path to binary).

### adding a language

Built-in: TypeScript/JavaScript, Python. Everything else (Rust, Go, C/C++, TOML, …)
downloads on first use into `~/.config/silence/grammars/` from GitHub release assets.

1. add optional pack metadata in `silence-langs` (`grammar_pack_id`, extensions)
2. add the pack to `silence-grammar-packs/build.rs` and release assets in `.github/workflows/release.yml`
3. wire `silence-grammars` `ensure()`; `every_grammar_loads_and_query_compiles`
   in `silence-strip-grammars` verifies query/ABI

Some files embed a second language. Astro's frontmatter is TypeScript, but the Astro
grammar returns it as one opaque node. `Lang::injections()` lists these sub-language
regions; silence re-parses each with the inner grammar and merges the comments back.
