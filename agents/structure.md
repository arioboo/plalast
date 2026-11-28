# Structure
note for agent: ...

Paths are added here as they are created:

| Path | Content |
|---|---|
| `memory.json` | The memory, the brain: `config`, `primary` and `data`; with the params of `params.yaml`, every `.md` in the root and in `agents/` is generated from it (see `agents/BUILD.md`) |
| `params.yaml` | Parameters: the agent placeholder (line 2 of every doc) and the interruptores (`levers`) of document sections and modes |
| `Cargo.toml` | Rust crate with four tools: `pla-build`, `pla-recall`, `pla-lint` and `pla-audit`, plus Verus verification (see `agents/BUILD.md`, `agents/guides/verus.md`) |
| `src/` | Rust code: `lib.rs` (shared memory access and checks), `pdf.rs` (guide renderer), `verified.rs` (Verus-verified code), `bin/build.rs`, `bin/recall.rs`, `bin/lint.rs`, `bin/audit.rs` |
| `tests/` | Integration tests: `tests/wiring.rs` runs the four tools against this repo's memory |
| `.secrets/` | Secrets: git-ignored except `.gitkeep`; values are referenced by environment variables, never written in other files |
| `agents/` | Single, provider-neutral source. Its `.md` files (`AGENTS.md`, `CLAUDE.md`, `PROCEDURES.md`, `BUILD.md`, `FEATURES.md`, `LOG.md`, `INDEX.md`) are generated; its subfolders hold hand-written modules |
| `agents/skills/<name>/SKILL.md` | Skills, one folder each |
| `agents/subagents/<name>.md` | Subagents |
| `agents/rules/*.md` | Modular rules by topic |
| `agents/guides/<name>.md` | Guides: hand-written .md sources and the .pdf that `pla-build` renders from each |
| `agents/hooks/` | Hook scripts: `protect-generated.sh` (Claude Code) and `pre-commit` (git); their config is `config.hooks` |
| `agents/mcp/<server>.json` | One MCP connector per file |
| `.claude/` | Claude Code adapter: `pla-build` links each module of `agents/` one by one, as `config.paths.modules.<kind>.links` says |
| `modules/` | Folder for links to the external modules of `primary.modules` (none is built now) |
| `tmux.conf` | tmux config for the `plalast` session, sourced on top of `~/.tmux.conf`; its header comments hold the commands to start it |
