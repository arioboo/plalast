# plalast - The Living Creature
note for the agent: this file is generated via the brain (memory.json) and binaries (build.rs)

This repo is a public and living creature. Turn on your VPS or machine, connect your agents, code it, teach it something, and make a pull request on Github.

The servers will be 24/7 on. This is a personal project on autonomous coding and living information systems, but everyone is welcome in.

## Premisas - estáticas

- Single source: `memory.json` is the brain, with exactly three primary keys (`config`, `primary`, `data`; see the table below), and `params.yaml` holds only the interruptores. Every `.md` in the repo root and in `agents/` is generated from them: edit only those two files and run `cargo run --release --bin pla-build` (see `agents/BUILD.md`). Modules in `agents/` subfolders are hand-written. Never edit generated files by hand.
- Secrets: they live only in `.secrets/` at the repo root, which git ignores except its `.gitkeep`. Code and configs reference them by environment variable; their values never go into other files, the chat or external services.
- Specific formats: markdown for information, json for the memory (`memory.json`), yaml for the interruptores (`params.yaml`), bash for system scripts (setup, links, servers) and Rust for everything else (see Premisas - dinámicas).
- No duplication. Only one way of functional working, one memory file and one params file, symbolic links where matters, only one zettelkasten vault, etc.
- Open source: everyone can collaborate with it (see `README.md` at the repo root for the contribution flow)
- Pareto rule: 20% of the effort gives 80% of the result; when unsure, assume a similar split. Propose that 20% first, and more when it is for best practices. It also rules every description: `descr_+` (what a key is, the positive way) and `descr_-` (what it is not, the negative way) each say the essential 20% that carries 80% of the meaning.

The memory (`memory.json`) has exactly three primary keys, each saying what it is (`descr_+`) and what it is not (`descr_-`):

| Key | Is (descr_+) | Is not (descr_-) |
|---|---|---|
| `config` | What the tools need to run the repo: repository, branches, paths, audit, lint and hooks. | Not text for the docs (primary), not what gets built (data), not interruptores (`params.yaml`), not secrets (`.secrets/`). |
| `primary` | What makes the docs: identity, texts, definitions, commands, features, log, index, modes and the schema of the memory itself. | Not paths or tool settings (config), not what gets built on disk (data), not hand-written code or modules. |
| `data` | What gets built: every path of the repo (`structure`) and the shape of every generated doc (`content`). | Not prose (texts live in primary and are pointed at with `ref`), not settings (config). |

## Premisas - dinámicas

Brain: `memory.json` is the brain of plalast, the place everything is read from and written to. Everything in this file, and every other generated file (every `.md` in the root and in `agents/`, `.gitignore`, `.claude/settings.json`), is rebuilt from it byte by byte, with the interruptores of `params.yaml`.

Write order: delete > modify/edit > add. Before adding anything, first delete what is no longer needed, then modify or edit what exists, and add only what neither covers. This forces a kind of autonomous retraining of the memory, as long as the Pareto rule holds in each case.

The current state of the repo: it changes as the repo grows, so check it against the repo when in doubt.

Map: `agents/structure.md` (where each path lives) and `cargo run --release --bin pla-recall -- [term]`.

Modes (`primary.mode`, each with its interruptor in `params.levers.mode`):

- Mode `ultra_conservative` (on_demand): read `agents/mode-ultra_conservative.md` when the user turns it on, or a change is risky, irreversible or outward-facing.

Repo tool (details in `agents/BUILD.md`), from anywhere inside the repo:

- `cargo run --release --bin pla-build`: regenerate docs, folders and links from `memory.json` and `params.yaml`
- `cargo run --release --bin pla-build -- --check`: change nothing, fail if the repo drifted from the memory
- `cargo run --release --bin pla-build -- --guard`: Claude Code hook: deny hand edits of generated files
- `cargo run --release --bin pla-recall -- [term]`: print the repo map, or search it for `term`
- `cargo run --release --bin pla-lint -- [memory] [modules] [rust]`: check that the memory, the hand-written modules and the Rust code are well formed
- `cargo run --release --bin pla-audit -- [secrets] [deps] [repo]`: look for secrets and personal data, vulnerable dependencies, broken links and branches out of sync
- `git config core.hooksPath agents/hooks`: once per clone, so git's pre-commit hook refuses commits while `pla-build --check` fails
- `cargo install cargo-audit --locked`: once per machine, so `pla-audit deps` checks `Cargo.lock` against RustSec instead of skipping it

Code:

- Rust: `cargo build`, `cargo test`, `cargo test <test_name>` (single test), `cargo clippy -- -D warnings`, `cargo fmt --check`
- Verus: `cargo verus verify` (formal verification of the `verus!` code; install and usage in `agents/guides/verus.md`)
- Lean 4: `lake build` (proofs of a Lean project; install and usage in `agents/guides/lean.md`)
- Dafny: `dafny verify <file>.dfy` (verification of a Dafny file; install and usage in `agents/guides/dafny.md`)

Tools and languages in use:

- Rust: Default language for all programs and tools, with possibility to add Verus for verification.
- Lean 4: For formal verification and expandable fact base.
- Dafny: For software verification and default LLM-programming

Candidates (possible, not in use yet):

- Typescript: Best for web, UI and web scripting/crawling

## Buenas prácticas

- Interruptor: every document section and every mode has one in `params.levers` (also called a memory lever), with three values: `enabled` (always in the agent's memory), `on_demand` (its own file, read when needed; the default when none is set) or `disabled` (not built). Every new section or mode gets one when its default is not the right one.
- File changes: when the user asks to touch any file, make the change in its section of `memory.json` (or `params.yaml` for interruptores) and rebuild with `pla-build` (the Rust build tool); never edit the output directly. Only the hand-written Rust code and the modules in `agents/` subfolders are edited in place.
- Log: after every commit, merge or new branch, add an entry `{date, branch, commit, kind, descr}` to `primary.log` (a commit cannot hold its own hash, so it goes in the next commit) and rebuild; `agents/LOG.md` shows it.
- Zettelkasten markdown spirit. A lot of things in the web are garbage, so notes in the agent-zettelkasten use CDU (Universal Decimal Classification) notation: the file name starts with the CDU code, then a kebab-case title, e.g. `004.8-llm-agents.md` (004.8 = artificial intelligence). The infrastructure will be up.
- Lean memory: always-on docs hold only what an agent cannot learn by reading the repo; maps, overviews and reference material are `on_demand`.
- Every rule earns its place: add one only when an agent repeats a mistake, and only if the agent could not find it in the repo; `pla-lint` keeps always-on docs under `config.lint.max_lines`.
- Enforce, don't ask: rules that must never bend (never edit generated files, docs in sync with `memory.json`) are hooks or checks, not just instructions.

## Preferencias

- Naming: when the user says "la memoria", "el contexto" or "el json" (memory, context, the json), they mean `memory.json`.
- Using languages LLM-friendly like Rust, Python or Go is desirable. Also, using LLM programming-specific languages is a tool to proceed with caution, but desirable at some point.
- Formal verification is desirable, but very expensive. Watch out and connect to others' ideas instead of reinventing the wheel
