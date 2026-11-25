# BUILD
note for the agent: this file is generated via the brain (config.json) and binaries (build.rs)

`config.json` is the brain of this repo: every `.md` file in the repo root and in `agents/` is generated from it, together with folders, symlinks and `.gitignore`. Modules in the `agents/` subfolders (skills, subagents, rules, hooks, mcp, memory) are hand-written and are not generated. Never edit a generated file: edit `config.json` and rebuild.

## Commands

| Command | What it does |
|---|---|
| `cargo run --release --bin pla-build` | Builds the repo from `config.json`. Idempotent: only writes what changed. |
| `cargo run --release --bin pla-build -- --check` | Changes nothing; exits non-zero if the repo drifted from `config.json` (stale file, missing link, stray `.md`). |
| `cargo run --release --bin pla-recall` | Prints the map: every path in `structure` with its type and description. |
| `cargo run --release --bin pla-recall -- <term>` | Searches paths, descriptions and document text for `<term>` (case-insensitive), naming the file and section of each hit. |
| `cargo run --release --bin pla-lint` | Checks that `config.json` is consistent and fully documented in `primary.schema`, that hand-written modules have the right shape (skill folders with `SKILL.md`, frontmatter `name` and `description`), that guides have their source, and that `cargo fmt`, `cargo clippy` and, when Verus is installed, `cargo verus verify` pass. Pass `config`, `modules` or `rust` to run only some areas. |
| `cargo run --release --bin pla-audit` | Looks for keys, tokens, emails and home-folder paths in what git would commit, files committed inside secret folders, known vulnerabilities in `Cargo.lock` (with `cargo-audit`), broken links, large files, uncommitted changes and branches out of sync. Pass `secrets`, `deps` or `repo` to run only some areas; `config.audit` holds its settings. |

Run them from anywhere inside the repo: the tool walks up to the folder that holds `config.json`. Symlinks need a Unix system (Linux or macOS). `pla-build` never replaces a real file or folder with a link, and never deletes anything except broken module links.

## config.json

It has exactly three top-level keys: `config`, what the tools need to run the repo (git, settings, paths); `primary`, what makes the docs (identity, texts, definitions, levers); and `data`, what gets built. Any other top-level key is an error.

| Key | Meaning |
|---|---|
| `config` | What the tools need to run the repo: git, tool settings and every path. |
| `config.repository` | Remote repository of the project. |
| `config.branches` | `integration` (where work is committed) and `stable` (what receives merges); `pla-audit` checks they are in sync. |
| `config.audit` | Settings of `pla-audit`: `allow` (exact strings or repo paths that are never reported, such as a public contact email) and `max_file_kb` (larger files are reported). |
| `config.paths` | Every path the tools use: `docs`, `on_demand`, `zettelkasten` and `modules`. |
| `config.paths.docs` | Folders whose `.md` files are docs (`.` is the repo root). Every `.md` directly inside them must be generated. |
| `config.paths.on_demand` | Folder where `on_demand` sections are written as `<id>.md`. |
| `config.paths.zettelkasten` | Path of the vault, relative to the repo root (keep it relative: an absolute path leaks your home folder). The `zettelkasten` link targets it. |
| `config.paths.modules` | Per module kind (skills, agents, rules, connectors, guides): `dir` (where its modules are written) and `links` (provider -> folder that gets one symlink per module). |
| `primary` | What makes the docs: project identity, texts, definitions, levers, features, log, index and this schema. |
| `primary.project` | Identity: `name`, `title`, `plalast_logo` (path of the project logo) and `purpose`. |
| `primary.docs` | Everything about the docs, the markdown files about this project and how it operates. |
| `primary.docs.definition` | What counts as a doc, in words. |
| `primary.docs.agent_note` | The note every generated doc carries on its second line, so an agent reading the file knows not to edit it by hand. Inserted by the `agent_note` block. |
| `primary.docs.levers` | Memory lever of every document section, as `<doc>` -> `<section id>` -> value (see Memory levers). |
| `primary.docs.on_demand` | How `on_demand` sections are pointed at: the heading of the pointer list (`heading`) and the pointer line (`pointer`, with `{path}`, `{title}` and `{when}`). Their folder is `config.paths.on_demand`. |
| `primary.agents-md` | The text of agents/AGENTS.md, split into fields; `data.content.agents_md` gives the doc its shape (sections, levers, agent note) and points at these fields with `ref`. |
| `primary.agents-md.title` | Title of AGENTS.md (its first line). |
| `primary.agents-md.intro` | Paragraphs under the title. |
| `primary.agents-md.good_practices` | Rules every agent follows (the Best practices section). |
| `primary.agents-md.commands` | The Commands section: `tool_intro` and `tool` (the pla-* tools), `code_intro` and `code` (cargo and verifiers). |
| `primary.agents-md.tools_and_languages` | Languages and verifiers in use. |
| `primary.agents-md.candidates` | Languages that may be used later. |
| `primary.features` | Features of the project, as `feature_<n>` -> `{descr, status}` (both strings). |
| `primary.log` | Project log: entries `{date, branch, commit, kind, descr}` in the order they happened; `kind` is `commit`, `merge` or `branch`. A commit cannot hold its own hash, so each one is logged in the next commit. LOG.md shows them newest first; `pla-audit` warns about commits missing from it. |
| `primary.index` | Project index (empty for now; its format is not defined yet). |
| `primary.skills` | Skills: `definition`; its folder and provider links are in `config.paths.modules`. |
| `primary.agents` | Subagents: `definition`; its folder and provider links are in `config.paths.modules`. |
| `primary.rules` | Rules: `definition`; its folder and provider links are in `config.paths.modules`. |
| `primary.connectors` | MCP connectors: `definition`; its folder and provider links are in `config.paths.modules`. `pla-lint` checks each file's neutral format. |
| `primary.guides` | Guides: `definition` and `render` (`pdf`: each .md is rendered to a .pdf by `pla-build`); folder and links in `config.paths.modules.guides`. |
| `primary.zettelkasten` | The zettelkasten vault to use: its `definition`; the path is `config.paths.zettelkasten`. |
| `primary.zettelkasten.definition` | What the vault is, in words. |
| `primary.schema` | This documentation of the config format; `BUILD.md` renders its tables. |
| `data` | What gets built. |
| `data.structure` | Every path, in order: `path`, `type`, `description`; optional `table` and `label` (its row in the Structure table), `gitkeep`, `secret`, `target` and `build`. |
| `data.content` | One document per key: `head` (blocks before the first section) and `sections` (`id`, `title`, `when`, `blocks`). A `file` entry with `build: "<key>"` is rendered from it. |

## Memory levers

Every section of every document has a lever in `primary.docs.levers.<doc>.<section id>`. A missing lever, a lever for a section that does not exist, or a value outside this table stops the build.

| Value | Effect |
|---|---|
| `enabled` | Rendered inside its document as `## <title>`: always in the agent's memory. |
| `on_demand` | Rendered to `<config.paths.on_demand>/<id>.md`; the document keeps only a pointer line under the on_demand heading, and the agent reads the file when `when` applies. |
| `disabled` | Not rendered anywhere. |

Switching a section from `on_demand` to `disabled` leaves its old file behind: `pla-build` reports it as a stray `.md` and fails until you delete it.

## Structure entry types

| Type | Build action |
|---|---|
| `dir` | Creates the folder; `gitkeep: true` adds an empty `.gitkeep`; `secret: true` git-ignores its contents except `.gitkeep`. |
| `symlink` | Creates a link to `target`, relative to the link's folder; `target_ref` (a dotted config path such as `config.paths.zettelkasten`) can replace `target`. |
| `file + build` | Renders `data.content.<build>`; `build: "gitignore"` renders `.gitignore` from the secret and ignored entries. |
| `file + build: null` | Hand-written code such as `Cargo.toml` or `src/`; never an `.md` in the root or `agents/`. Left as is. |
| `ignored` | Not created; listed in `.gitignore`. |
| `planned` | Not created; listed as planned in the Structure table. |

## Content blocks

| Block | Markdown |
|---|---|
| `heading` | `#` repeated `level` times, then `text`; `ref` (a dotted config path) can replace `text` |
| `paragraph` | `text` as is; `ref` (a dotted config path) can replace `text` |
| `raw` | `text` as is; `ref` (a dotted config path) can replace `text` |
| `list` | one `- item` line per item; `ref` (a dotted config path to a list of strings) can replace `items` |
| `ordered_list` | `1. item`, `2. item`, ...; `ref` (a dotted config path to a list of strings) can replace `items` |
| `code` | a fenced block with `lang` and `text` |
| `table` | `header` and `rows` as a markdown table |
| `structure_table` | the Structure table, from the `data.structure` entries that have `table` |
| `schema` | a table of `primary.schema.<part>` with the given `header` |
| `image` | `![alt](src)`, or an HTML `<img>` when `width` is set; `ref` (a dotted config path such as `primary.project.plalast_logo`) can replace `src` |
| `agent_note` | the text of `primary.docs.agent_note`, on the line right after the first one (no blank line); every doc has it as the second block of its `head` |
| `log` | a table of `primary.log`, newest first: date, branch, commit, kind and description |

Each section is rendered as `## <title>` followed by its blocks. Blocks are separated by one blank line, except `agent_note`, which goes right under the first line; every file ends with one newline.

## Adding a module

Write the module in its folder (for example `agents/skills/<name>/SKILL.md`) and run `pla-build`: each provider in `config.paths.modules.<kind>.links` gets one link for it (`.claude/skills/<name>` -> `../../agents/skills/<name>`), never a whole folder. Links of deleted modules are removed on the next build.

Guides are modules too: write `agents/guides/<name>.md` and run `pla-build`, which renders `agents/guides/<name>.pdf` from it (headings, paragraphs, lists and code blocks; inline markup is flattened). The PDF is deterministic, so `pla-build --check` reports a stale one.

## Removing a path

Deleting an entry from `config.json` does not delete it from disk. `pla-build --check` reports leftover `.md` files in the root and `agents/`; delete them once you have confirmed they are no longer needed.
