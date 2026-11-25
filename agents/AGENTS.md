# plalast - The Living Creature

This repo is a public and living creature. Turn on your VPS or machine, connect your agents, code it, teach it something, and make a pull request on Github.

The servers will be 24/7 on. This is a personal project on autonomous coding and living information systems, but everyone is welcome in.

## Best practices

- Single source: `config.json` is the brain. Every `.md` in the repo root and in `agents/` is generated from it, so edit only `config.json` and run `cargo run --release --bin plalast-build` (see `agents/BUILD.md`). Modules in `agents/` subfolders are hand-written. Never edit generated files by hand.
- Memory levers: every document section in `config.json` has a lever in `primary.levers`: `enabled` (always in the agent's memory), `on_demand` (its own file, read when needed) or `disabled` (not built). Every new section gets one.
- Zettelkasten markdown spirit. A lot of things in the web are garbage, so notes in the agent-zettelkasten use CDU (Universal Decimal Classification) notation: the file name starts with the CDU code, then a kebab-case title, e.g. `004.8-llm-agents.md` (004.8 = artificial intelligence). The infrastructure will be up.
- Specific formats: markdown for information, json for configs, bash for system scripts (setup, links, servers) and Rust for everything else (see Tools and languages).
- No duplication. Only one way of functional working, only one config file, symbolic links where matters, only one zettelkasten vault, etc.
- Using languages LLM-friendly like Rust, Python or Go is desirable. Also, using LLM programming-specific languages is a tool to proceed with caution, but desirable at some point.
- Formal verification is desirable, but very expensive. Watch out and connect to others' ideas instead of reinventing the wheel
- Open source: everyone can collaborate with it (see `README.md` at the repo root for the contribution flow)

## Structure

Paths are added here as they are created:

| Path | Content |
|---|---|
| `config.json` | The brain: every `.md` in the root and in `agents/`, plus folders, links and `.gitignore`, is generated from it (see `agents/BUILD.md`) |
| `Cargo.toml` | Rust crate with two tools, `plalast-build` and `plalast-recall` (see `agents/BUILD.md`) |
| `src/` | Rust code: `lib.rs` (shared config access), `bin/build.rs`, `bin/recall.rs` |
| `agents/` | Single, provider-neutral source. Its `.md` files (`AGENTS.md`, `CLAUDE.md`, `BUILD.md`) are generated; its subfolders hold hand-written modules |
| `agents/skills/<name>/SKILL.md` | Skills, one folder each |
| `agents/subagents/<name>.md` | Subagents |
| `agents/rules/*.md` | Modular rules by topic |
| `agents/hooks/` | Hook scripts (their config lives in each provider's settings) |
| `agents/mcp/<server>.json` | One MCP connector per file |
| `agents/memory/` | Persistent memory (json) |
| `agents/.secrets/` | Git-ignored except `.gitkeep` |
| `.claude/` | Claude Code adapter: `plalast-build` links each module of `agents/` one by one (`skills/`, `agents/` for subagents, `rules/`) |
| `zettelkasten` | Symlink to the single zettelkasten vault (CDU-named notes), outside this repo |

## Commands

Repo tool (details in `agents/BUILD.md`), from anywhere inside the repo:

- `cargo run --release --bin plalast-build`: regenerate docs, folders and links from `config.json`
- `cargo run --release --bin plalast-build -- --check`: change nothing, fail if the repo drifted from `config.json`
- `cargo run --release --bin plalast-recall -- [term]`: print the repo map, or search it for `term`

Code:

- Rust: `cargo build`, `cargo test`, `cargo test <test_name>` (single test), `cargo clippy -- -D warnings`, `cargo fmt --check`
- Verus: `verus <file>.rs`
- Lean 4: `lake build`
- Dafny: `dafny verify <file>.dfy`

## Tools and languages

- Rust: Default language for all programs and tools, with possibility to add Verus for verification.
- Lean 4: For formal verification and expandable fact base.
- Dafny: For software verification and default LLM-programming

## Candidates (possible, not in use yet)

- Typescript: Best for web, UI and web scripting/crawling
