# plalast - The Living Creature

This repo is a public and living creature. Turn on your VPS or machine, connect your agents, code it, teach it something, and make a pull request on Github.

The servers will be 24/7 on. This is a personal project on autonomous coding and living information systems, but everyone is welcome in.

## Best practices

- Single source: `config.json` is the brain. Every `.md` in the repo root and in `agents/` is generated from it, so edit only `config.json` and run `cargo run --release --bin pla-build` (see `agents/BUILD.md`). Modules in `agents/` subfolders are hand-written. Never edit generated files by hand.
- Naming: when the user says "la memoria", "el contexto" or "el json" (memory, context, the json), they mean `config.json`.
- Docs: "docs" are all the markdown files about this project and how it operates; their definition, folders, levers and on_demand settings live in `primary.docs`.
- File changes: when the user asks to touch any file, make the change in its section of `config.json` and rebuild with `pla-build` (the Rust build tool); never edit the output directly. Only the hand-written Rust code and the modules in `agents/` subfolders are edited in place.
- Secrets: they live only in `.secrets/` at the repo root, which git ignores except its `.gitkeep`. Code and configs reference them by environment variable; their values never go into other files, the chat or external services.
- Memory levers: every document section in `config.json` has a lever in `primary.docs.levers`: `enabled` (always in the agent's memory), `on_demand` (its own file, read when needed) or `disabled` (not built). Every new section gets one.
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
| `Cargo.toml` | Rust crate with four tools: `pla-build`, `pla-recall`, `pla-lint` and `pla-audit`, plus Verus verification (see `agents/BUILD.md`, `agents/guides/verus.md`) |
| `src/` | Rust code: `lib.rs` (shared config access and checks), `pdf.rs` (guide renderer), `verified.rs` (Verus-verified code), `bin/build.rs`, `bin/recall.rs`, `bin/lint.rs`, `bin/audit.rs` |
| `tests/` | Integration tests: `tests/wiring.rs` runs the four tools against this repo's memory |
| `.secrets/` | Secrets: git-ignored except `.gitkeep`; values are referenced by environment variables, never written in other files |
| `agents/` | Single, provider-neutral source. Its `.md` files (`AGENTS.md`, `CLAUDE.md`, `BUILD.md`, `FEATURES.md`, `LOG.md`, `INDEX.md`) are generated; its subfolders hold hand-written modules |
| `agents/skills/<name>/SKILL.md` | Skills, one folder each |
| `agents/subagents/<name>.md` | Subagents |
| `agents/rules/*.md` | Modular rules by topic |
| `agents/guides/<name>.md` | Guides: hand-written .md sources and the .pdf that `pla-build` renders from each |
| `agents/hooks/` | Hook scripts (their config lives in each provider's settings) |
| `agents/mcp/<server>.json` | One MCP connector per file |
| `agents/memory/` | Persistent memory (json) |
| `.claude/` | Claude Code adapter: `pla-build` links each module of `agents/` one by one, as the `links` of `primary.skills`, `primary.agents` and `primary.rules` say |
| `zettelkasten` | Symlink to the single zettelkasten vault (CDU-named notes), outside this repo |

## Commands

Repo tool (details in `agents/BUILD.md`), from anywhere inside the repo:

- `cargo run --release --bin pla-build`: regenerate docs, folders and links from `config.json`
- `cargo run --release --bin pla-build -- --check`: change nothing, fail if the repo drifted from `config.json`
- `cargo run --release --bin pla-recall -- [term]`: print the repo map, or search it for `term`
- `cargo run --release --bin pla-lint -- [config] [modules] [rust]`: check that config.json, the hand-written modules and the Rust code are well formed
- `cargo run --release --bin pla-audit -- [secrets] [deps] [repo]`: look for secrets and personal data, vulnerable dependencies, broken links and branches out of sync

Code:

- Rust: `cargo build`, `cargo test`, `cargo test <test_name>` (single test), `cargo clippy -- -D warnings`, `cargo fmt --check`
- Verus: `cargo verus verify` (formal verification of the `verus!` code; install and usage in `agents/guides/verus.md`)
- Lean 4: `lake build`
- Dafny: `dafny verify <file>.dfy`

## Tools and languages

- Rust: Default language for all programs and tools, with possibility to add Verus for verification.
- Lean 4: For formal verification and expandable fact base.
- Dafny: For software verification and default LLM-programming

## Candidates (possible, not in use yet)

- Typescript: Best for web, UI and web scripting/crawling
