# BUILD
note for the agent: this file is generated via the brain (memory.json) and binaries (build.rs)

Single source: `memory.json` (texts, structure, content) and `params.yaml` (settings, paths, levers) are the brain. Every `.md` in the repo root and in `agents/` is generated from them: edit only those two files and run `cargo run --release --bin pla-build` (see `agents/BUILD.md`). Modules in `agents/` subfolders are hand-written. Never edit generated files by hand.

## Commands

| Command | What it does |
|---|---|
| `cargo run --release --bin pla-build` | Builds the repo from `memory.json` and `params.yaml`. Idempotent: only writes what changed. |
| `cargo run --release --bin pla-build -- --check` | Changes nothing; exits non-zero if the repo drifted from the memory (stale file, missing link, stray `.md`). |
| `cargo run --release --bin pla-build -- --guard` | Reads a Claude Code PreToolUse payload on stdin and denies Edit/Write on any file `pla-build` generates; `agents/hooks/protect-generated.sh` runs it. |
| `cargo run --release --bin pla-recall -- [term]` | Without `term`, prints the map: every path with its type and description, the module kinds, features, log, index, settings and levers. With `term`, searches all of it and the document text (case-insensitive), naming the file, section and lever of each hit. |
| `cargo run --release --bin pla-lint -- [memory] [modules] [rust]` | Checks that `memory.json` and `params.yaml` are consistent and fully documented in `primary.schema`, that hand-written modules have the right shape (skill folders with `SKILL.md`, frontmatter `name` and `description`), that guides have their source, and that `cargo fmt`, `cargo clippy` and, when Verus is installed, `cargo verus verify` pass. Pass `memory`, `modules` or `rust` to run only some areas. |
| `cargo run --release --bin pla-audit -- [secrets] [deps] [repo]` | Looks for keys, tokens, emails and home-folder paths in what git would commit, files committed inside secret folders, known vulnerabilities in `Cargo.lock` (with `cargo-audit`), broken links, large files, uncommitted changes and branches out of sync. Pass `secrets`, `deps` or `repo` to run only some areas; `params.config.audit` holds its settings. |
| `git config core.hooksPath agents/hooks` | Run once per clone: git then uses `agents/hooks`, whose `pre-commit` refuses a commit while `pla-build --check` fails. `pla-audit` warns when it is not set. |
| `cargo install cargo-audit --locked` | Run once per machine (it compiles from source and can take about 15 minutes): `pla-audit deps` then checks `Cargo.lock` against the RustSec advisory database. Without it, that check is skipped with a warning. |

Run them from anywhere inside the repo: the tool walks up to the folder that holds `memory.json`. Symlinks need a Unix system (Linux or macOS). `pla-build` never replaces a real file or folder with a link, and never deletes anything except broken module links.

## memory.json and params.yaml

The brain is two files. `memory.json` has exactly `primary` (what makes the docs: identity, texts, definitions) and `data` (what gets built). `params.yaml` has exactly `config` (what the tools need to run: git, settings, paths) and `levers` (the memory levers). The tools join them as `params`, `primary` and `data`, so a dotted path such as `params.config.paths.modules.agent_zettelkasten` works everywhere. Any other top-level key is an error.

| Key | Meaning |
|---|---|
| `params` | Parameters, from `params.yaml`: `config` and `levers`. |
| `params.config` | What the tools need to run the repo: git, tool settings and every path. |
| `params.config.PLALAST_DIR` | Where the repo lives: always `$PWD`, the repo root, which the tools find by walking up to `memory.json` and resolve at run time. No path is written, so the same value works on every machine and never leaks the user name. |
| `params.config.repository` | Remote repository of the project. |
| `params.config.branches` | `integration` (where work is committed) and `stable` (what receives merges); `pla-audit` checks they are in sync. |
| `params.config.audit` | Settings of `pla-audit`: `allow` (exact strings or repo paths that are never reported, such as a public contact email) and `max_file_kb` (larger files are reported). |
| `params.config.lint` | Settings of `pla-lint`: `always_on` (docs an agent loads every session) and `max_lines` (their combined line budget; over it, move sections to `on_demand`). |
| `params.config.hooks` | Hooks that enforce the rules: `dir` (scripts; also git's `core.hooksPath`, so `pre-commit` runs `pla-build --check`) and `claude` (Claude Code hooks, rendered into `.claude/settings.json`; the PreToolUse hook denies hand edits of generated files). |
| `params.config.paths` | Every path the tools use: `docs`, `on_demand` and `modules`. |
| `params.config.paths.docs` | Folders whose `.md` files are docs (`.` is the repo root). Every `.md` directly inside them must be generated. |
| `params.config.paths.on_demand` | Folder where `on_demand` sections are written as `<id>.md`. |
| `params.config.paths.modules` | Per module kind (skills, agents, rules, connectors, guides): `dir` (where its modules are written) and `links` (provider -> folder that gets one symlink per module). Per external module of `primary.modules` (such as `agent_zettelkasten`): its path, relative to `modules/` (or to the folder of a structure symlink whose `target_ref` points at it). Keep it relative: an absolute path leaks your home folder. It depends on where the repo and the module sit on the machine, so a machine with another layout changes the value. |
| `params.levers` | Memory lever of every document section, as `<doc>` -> `<section id>` -> value (see Memory levers). |
| `primary` | What makes the docs: project identity, texts, definitions, commands, features, log, index and this schema. |
| `primary.project` | Identity: `name`, `title` and `plalast_logo` (path of the project logo). |
| `primary.docs` | Everything about the docs, the markdown files about this project and how it operates. |
| `primary.docs.definition` | What counts as a doc, in words. |
| `primary.docs.agent_note` | The note every generated doc carries on its second line, so an agent reading the file knows not to edit it by hand. Inserted by the `agent_note` block. |
| `primary.docs.on_demand` | How `on_demand` sections are pointed at: the heading of the pointer list (`heading`) and the pointer line (`pointer`, with `{path}`, `{title}` and `{when}`). Their folder is `params.config.paths.on_demand`. |
| `primary.agents-md` | The AGENTS.md text that no other key holds, split into fields; `data.content.agents_md` gives the doc its shape and points at them with `ref`. |
| `primary.agents-md.intro` | Paragraphs under the title. |
| `primary.agents-md.static_premises` | Fixed premises: what the repo is and how it is built (the Premisas - estáticas section). |
| `primary.agents-md.dynamic_premises` | Texts of the Premisas - dinámicas section (`intro`, `map`, `tools_intro`, `candidates_intro`); its lists are `commands`, `tools_and_languages` and `candidates`. |
| `primary.agents-md.commands` | Commands of the Premisas - dinámicas section: intros (`tool_intro`, `code_intro`) and the `code` commands; the tool commands come from `primary.commands`. |
| `primary.agents-md.tools_and_languages` | Languages and verifiers in use. |
| `primary.agents-md.candidates` | Languages that may be used later. |
| `primary.agents-md.good_practices` | How every agent works in the repo (the Buenas prácticas section). |
| `primary.agents-md.preferences` | Preferences of the project owner (the Preferencias section). |
| `primary.features` | Features of the project, as `feature_<n>` -> `{descr, status}` (both strings). |
| `primary.log` | Project log: entries `{date, branch, commit, kind, descr}` in the order they happened; `kind` is `commit`, `merge` or `branch`. A commit cannot hold its own hash, so each one is logged in the next commit. LOG.md shows them newest first; `pla-audit` warns about commits missing from it. |
| `primary.index` | Project index (empty for now; its format is not defined yet). |
| `primary.skills` | Skills: `definition`; its folder and provider links are in `params.config.paths.modules`. |
| `primary.agents` | Subagents: `definition`; its folder and provider links are in `params.config.paths.modules`. |
| `primary.rules` | Rules: `definition`; its folder and provider links are in `params.config.paths.modules`. |
| `primary.connectors` | MCP connectors: `definition`; its folder and provider links are in `params.config.paths.modules`. `pla-lint` checks each file's neutral format. |
| `primary.guides` | Guides: `definition` and `render` (`pdf`: each .md is rendered to a .pdf by `pla-build`); folder and links in `params.config.paths.modules.guides`. |
| `primary.modules` | External modules that live outside this repo, by name (such as `agent_zettelkasten`): each has a `definition` (what it is, in words); its path is `params.config.paths.modules.<name>`. A symlink in `modules/` is built only if `data.structure` has one for it. |
| `primary.schema` | This documentation of the memory and params format; `BUILD.md` renders its tables. |
| `primary.commands` | The repo's commands, each `{cmd, summary, detail}`: AGENTS.md lists the summaries and BUILD.md the details. |
| `data` | What gets built. |
| `data.structure` | Every path, in order: `path`, `type`, `description`; optional `table` and `label` (its row in the Structure table), `gitkeep`, `secret`, `target`, `local` and `build`. |
| `data.content` | One document per key: `head` (blocks before the first section) and `sections` (`id`, `title`, `when`, `blocks`). A `file` entry with `build: "<key>"` is rendered from it. |

## Memory levers (params.yaml)

Every section of every document has a lever in `params.levers.<doc>.<section id>`. A missing lever, a lever for a section that does not exist, or a value outside this table stops the build.

| Value | Effect |
|---|---|
| `enabled` | Rendered inside its document as `## <title>`: always in the agent's memory. |
| `on_demand` | Rendered to `<params.config.paths.on_demand>/<id>.md`; the document keeps only a pointer line under the on_demand heading, and the agent reads the file when `when` applies. |
| `disabled` | Not rendered anywhere. |

Switching a section from `on_demand` to `disabled` leaves its old file behind: `pla-build` reports it as a stray `.md` and fails until you delete it.

## Structure entry types

| Type | Build action |
|---|---|
| `dir` | Creates the folder; `gitkeep: true` adds an empty `.gitkeep`; `secret: true` git-ignores its contents except `.gitkeep`. `path_ref` (a dotted path such as `params.config.paths.modules.skills.dir`) can replace `path`, and `label` may use `{path}`. |
| `symlink` | Creates a link to `target`, relative to the link's folder; `target_ref` (a dotted config path such as `params.config.paths.modules.agent_zettelkasten`) can replace `target`. `local: true` marks a link that depends on the machine's layout: it is git-ignored, built only where its target exists, and removed where it does not. |
| `file + build` | Renders `data.content.<build>`; `build: "gitignore"` renders `.gitignore` from the secret, ignored and local entries; `build: "claude_settings"` renders `.claude/settings.json` from `params.config.hooks.claude`. |
| `file + build: null` | Hand-written code such as `Cargo.toml` or `src/`; never an `.md` in the root or `agents/`. Left as is. |
| `ignored` | Not created; listed in `.gitignore`. |
| `planned` | Not created; listed as planned in the Structure table. |

## Content blocks

| Block | Markdown |
|---|---|
| `heading` | `#` repeated `level` times, then `text`; `ref` (a dotted config path) can replace `text` |
| `paragraph` | `text` as is; `ref` (a dotted config path) can replace `text` |
| `list` | one `- item` line per item; `ref` (a dotted path to a list) can replace `items`, and `item` (a template such as `{cmd}: {summary}`) renders a list of objects |
| `ordered_list` | `1. item`, `2. item`, ...; `ref` (a dotted config path to a list of strings) can replace `items` |
| `code` | a fenced block with `lang` and `text` |
| `table` | `header` and `rows` as a markdown table; or `ref` to a list of objects with `row` (one template per column) and optional `reverse` |
| `structure_table` | the Structure table, from the `data.structure` entries that have `table` |
| `schema` | a table of `primary.schema.<part>` with the given `header` |
| `image` | `![alt](src)`, or an HTML `<img>` when `width` is set; `ref` (a dotted config path such as `primary.project.plalast_logo`) can replace `src` |
| `agent_note` | the text of `primary.docs.agent_note`, on the line right after the first one (no blank line); every doc has it as the second block of its `head` |

Each section is rendered as `## <title>` followed by its blocks. Blocks are separated by one blank line, except `agent_note`, which goes right under the first line; every file ends with one newline.

## Adding a module

Write the module in its folder (for example `agents/skills/<name>/SKILL.md`) and run `pla-build`: each provider in `params.config.paths.modules.<kind>.links` gets one link for it (`.claude/skills/<name>` -> `../../agents/skills/<name>`), never a whole folder. Links of deleted modules are removed on the next build.

Guides are modules too: write `agents/guides/<name>.md` and run `pla-build`, which renders `agents/guides/<name>.pdf` from it (headings, paragraphs, lists and code blocks; inline markup is flattened). The PDF is deterministic, so `pla-build --check` reports a stale one.

## Removing a path

Deleting an entry from `memory.json` does not delete it from disk. `pla-build --check` reports leftover `.md` files in the root and `agents/`; delete them once you have confirmed they are no longer needed.

## On demand

- `agents/structure.md`: Structure. Read it when looking for where something lives or adding a path.
