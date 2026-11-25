# plalast - The Living Creature
note for the agent: this file is generated via the brain (memory.json) and binaries (build.rs)

This repo is a public and living creature. Turn on your VPS or machine, connect your agents, code it, teach it something, and make a pull request on Github.

The servers will be 24/7 on. This is a personal project on autonomous coding and living information systems, but everyone is welcome in.

## Best practices

- Single source: `memory.json` (texts, structure, content) and `params.yaml` (settings, paths, levers) are the brain. Every `.md` in the repo root and in `agents/` is generated from them: edit only those two files and run `cargo run --release --bin pla-build` (see `agents/BUILD.md`). Modules in `agents/` subfolders are hand-written. Never edit generated files by hand.
- Naming: when the user says "la memoria", "el contexto" or "el json" (memory, context, the json), they mean `memory.json`.
- File changes: when the user asks to touch any file, make the change in its section of `memory.json` (or `params.yaml` for settings and levers) and rebuild with `pla-build` (the Rust build tool); never edit the output directly. Only the hand-written Rust code and the modules in `agents/` subfolders are edited in place.
- Secrets: they live only in `.secrets/` at the repo root, which git ignores except its `.gitkeep`. Code and configs reference them by environment variable; their values never go into other files, the chat or external services.
- Memory levers: every document section has a lever in `params.levers`: `enabled` (always in the agent's memory), `on_demand` (its own file, read when needed) or `disabled` (not built). Every new section gets one.
- Log: after every commit, merge or new branch, add an entry `{date, branch, commit, kind, descr}` to `primary.log` (a commit cannot hold its own hash, so it goes in the next commit) and rebuild; `agents/LOG.md` shows it.
- Zettelkasten markdown spirit. A lot of things in the web are garbage, so notes in the agent-zettelkasten use CDU (Universal Decimal Classification) notation: the file name starts with the CDU code, then a kebab-case title, e.g. `004.8-llm-agents.md` (004.8 = artificial intelligence). The infrastructure will be up.
- Specific formats: markdown for information, json for the memory (`memory.json`), yaml for parameters (`params.yaml`), bash for system scripts (setup, links, servers) and Rust for everything else (see Tools and languages).
- No duplication. Only one way of functional working, one memory file and one params file, symbolic links where matters, only one zettelkasten vault, etc.
- Using languages LLM-friendly like Rust, Python or Go is desirable. Also, using LLM programming-specific languages is a tool to proceed with caution, but desirable at some point.
- Formal verification is desirable, but very expensive. Watch out and connect to others' ideas instead of reinventing the wheel
- Open source: everyone can collaborate with it (see `README.md` at the repo root for the contribution flow)
- Pareto rule: The 20% of effort in an issue makes the 80% of results, and if not, assume a similar proportion. You'll propose it, and more if it is for best practices
- Lean memory: always-on docs hold only what an agent cannot learn by reading the repo; maps, overviews and reference material are `on_demand`.
- Every rule earns its place: add one only when an agent repeats a mistake, and only if the agent could not find it in the repo; `pla-lint` keeps always-on docs under `params.config.lint.max_lines`.
- Enforce, don't ask: rules that must never bend (never edit generated files, docs in sync with `memory.json`) are hooks or checks, not just instructions.

## Commands

Repo tool (details in `agents/BUILD.md`), from anywhere inside the repo:

- `cargo run --release --bin pla-build`: regenerate docs, folders and links from `memory.json` and `params.yaml`
- `cargo run --release --bin pla-build -- --check`: change nothing, fail if the repo drifted from the memory
- `cargo run --release --bin pla-build -- --guard`: Claude Code hook: deny hand edits of generated files
- `cargo run --release --bin pla-recall -- [term]`: print the repo map, or search it for `term`
- `cargo run --release --bin pla-lint -- [memory] [modules] [rust]`: check that the memory, the hand-written modules and the Rust code are well formed
- `cargo run --release --bin pla-audit -- [secrets] [deps] [repo]`: look for secrets and personal data, vulnerable dependencies, broken links and branches out of sync
- `git config core.hooksPath agents/hooks`: once per clone, so git's pre-commit hook refuses commits while `pla-build --check` fails

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

## On demand

- `agents/structure.md`: Structure. Read it when looking for where something lives or adding a path.
