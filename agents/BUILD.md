# BUILD

`config.json` is the brain of this repo: every `.md` file in the repo root and in `agents/` is generated from it, together with folders, symlinks and `.gitignore`. Modules in the `agents/` subfolders (skills, subagents, rules, hooks, mcp, memory) are hand-written and are not generated. Never edit a generated file: edit `config.json` and rebuild.

## Commands

| Command | What it does |
|---|---|
| `cargo run --release --bin pla-build` | Builds the repo from `config.json`. Idempotent: only writes what changed. |
| `cargo run --release --bin pla-build -- --check` | Changes nothing; exits non-zero if the repo drifted from `config.json` (stale file, missing link, stray `.md`). |
| `cargo run --release --bin pla-recall` | Prints the map: every path in `structure` with its type and description. |
| `cargo run --release --bin pla-recall -- <term>` | Searches paths, descriptions and document text for `<term>` (case-insensitive), naming the file and section of each hit. |
| `cargo run --release --bin pla-lint` | Checks that `config.json` is consistent and fully documented in `primary.schema`, that hand-written modules have the right shape (skill folders with `SKILL.md`, frontmatter `name` and `description`), and that `cargo fmt` and `cargo clippy` pass. Pass `config`, `modules` or `rust` to run only some areas. |
| `cargo run --release --bin pla-audit` | Looks for keys, tokens, emails and home-folder paths in what git would commit, files committed inside secret folders, known vulnerabilities in `Cargo.lock` (with `cargo-audit`), broken links, large files, uncommitted changes and branches out of sync. Pass `secrets`, `deps` or `repo` to run only some areas; `primary.audit` holds its settings. |

Run them from anywhere inside the repo: the tool walks up to the folder that holds `config.json`. Symlinks need a Unix system (Linux or macOS). `pla-build` never replaces a real file or folder with a link, and never deletes anything except broken adapter links.

## config.json

It has exactly two top-level keys: `primary`, the control panel (what to build and how), and `data`, what gets built. Any other top-level key is an error.

| Key | Meaning |
|---|---|
| `primary` | Control panel: what to build and how. |
| `primary.project` | Identity: `name`, `title`, `plalast_logo` (path of the project logo), `purpose`, `repository` and `branches`. |
| `primary.levers` | Memory lever of every document section, as `<doc>` -> `<section id>` -> value (see Memory levers). |
| `primary.on_demand` | Where `on_demand` sections go (`dir`), the heading of the pointer list (`heading`) and the pointer line (`pointer`, with `{path}`, `{title}` and `{when}`). |
| `primary.adapters` | Per provider, which `agents/` folders get one symlink per module (`source`) and where (`dest`). |
| `primary.audit` | Settings of `pla-audit`: `allow` (exact strings or repo paths that are never reported, such as a public contact email) and `max_file_kb` (larger files are reported). |
| `primary.schema` | This documentation of the config format; `BUILD.md` renders its tables. |
| `data` | What gets built. |
| `data.structure` | Every path, in order: `path`, `type`, `description`; optional `table` and `label` (its row in the Structure table), `gitkeep`, `secret`, `target` and `build`. |
| `data.content` | One document per key: `head` (blocks before the first section) and `sections` (`id`, `title`, `when`, `blocks`). A `file` entry with `build: "<key>"` is rendered from it. |

## Memory levers

Every section of every document has a lever in `primary.levers.<doc>.<section id>`. A missing lever, a lever for a section that does not exist, or a value outside this table stops the build.

| Value | Effect |
|---|---|
| `enabled` | Rendered inside its document as `## <title>`: always in the agent's memory. |
| `on_demand` | Rendered to `<on_demand.dir>/<id>.md`; the document keeps only a pointer line under the on_demand heading, and the agent reads the file when `when` applies. |
| `disabled` | Not rendered anywhere. |

Switching a section from `on_demand` to `disabled` leaves its old file behind: `pla-build` reports it as a stray `.md` and fails until you delete it.

## Structure entry types

| Type | Build action |
|---|---|
| `dir` | Creates the folder; `gitkeep: true` adds an empty `.gitkeep`; `secret: true` git-ignores its contents except `.gitkeep`. |
| `symlink` | Creates a link to `target`, relative to the link's folder. |
| `file + build` | Renders `data.content.<build>`; `build: "gitignore"` renders `.gitignore` from the secret and ignored entries. |
| `file + build: null` | Hand-written code such as `Cargo.toml` or `src/`; never an `.md` in the root or `agents/`. Left as is. |
| `ignored` | Not created; listed in `.gitignore`. |
| `planned` | Not created; listed as planned in the Structure table. |

## Content blocks

| Block | Markdown |
|---|---|
| `heading` | `#` repeated `level` times, then `text` |
| `paragraph` | `text` as is |
| `raw` | `text` as is |
| `list` | one `- item` line per item |
| `ordered_list` | `1. item`, `2. item`, ... |
| `code` | a fenced block with `lang` and `text` |
| `table` | `header` and `rows` as a markdown table |
| `structure_table` | the Structure table, from the `data.structure` entries that have `table` |
| `schema` | a table of `primary.schema.<part>` with the given `header` |
| `image` | `![alt](src)`, or an HTML `<img>` when `width` is set; `ref` (a dotted config path such as `primary.project.plalast_logo`) can replace `src` |

Each section is rendered as `## <title>` followed by its blocks. Blocks are separated by one blank line and every file ends with one newline.

## Adding a module

Write the module in its folder (for example `agents/skills/<name>/SKILL.md`) and run `pla-build`: each adapter links it on its own (`.claude/skills/<name>` -> `../../agents/skills/<name>`), never a whole folder. Links of deleted modules are removed on the next build.

## Removing a path

Deleting an entry from `config.json` does not delete it from disk. `pla-build --check` reports leftover `.md` files in the root and `agents/`; delete them once you have confirmed they are no longer needed.
