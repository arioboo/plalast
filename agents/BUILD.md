# BUILD

`config.json` is the brain of this repo: every `.md` file in the repo root and in `agents/` is generated from it, together with folders, symlinks and `.gitignore`. Modules in the `agents/` subfolders (skills, subagents, rules, hooks, mcp, memory) are hand-written and are not generated. Never edit a generated file: edit `config.json` and rebuild.

## Commands

| Command | What it does |
|---|---|
| `cargo run --release -- build` | Builds the repo from `config.json`. Idempotent: only writes what changed. |
| `cargo run --release -- build --check` | Changes nothing; exits non-zero if the repo drifted from `config.json` (stale file, missing link, stray `.md`). |
| `cargo run --release -- recall` | Prints the map: every path in `structure` with its type and description. |
| `cargo run --release -- recall <term>` | Searches paths, descriptions and document text for `<term>` (case-insensitive), naming the file and section of each hit. |

Run them from anywhere inside the repo: the tool walks up to the folder that holds `config.json`. Symlinks need a Unix system (Linux or macOS). `build` never replaces a real file or folder with a link, and never deletes anything except broken adapter links.

## config.json

- `project`: name, title, repository and branches.
- `structure`: every path, in order, with `type` and `description`; `table` (and optionally `label`) gives its row in the Structure table of `agents/AGENTS.md`.
- `adapters`: per provider, which `agents/` folders get one symlink per module, and where.
- `content.<name>`: the blocks of one generated document; a `file` entry with `build: "<name>"` is rendered from it.

## Structure entry types

| Type | Build action |
|---|---|
| `dir` | Creates the folder; `gitkeep: true` adds an empty `.gitkeep`; `secret: true` git-ignores its contents except `.gitkeep`. |
| `symlink` | Creates a link to `target`, relative to the link's folder. |
| `file` with `build` | Renders `content.<build>`; `build: "gitignore"` renders `.gitignore` from the secret and ignored entries. |
| `file` with `build: null` | Hand-written code such as `Cargo.toml` or `src/`; never an `.md` in the root or `agents/`. Left as is. |
| `ignored` | Not created; listed in `.gitignore`. |
| `planned` | Not created; listed as planned in the Structure table. |

## Content blocks

| Block | Markdown |
|---|---|
| `heading` | `#` repeated `level` times, then `text` |
| `paragraph`, `raw` | `text` as is |
| `list` | one `- item` line per item |
| `ordered_list` | `1. item`, `2. item`, ... |
| `code` | a fenced block with `lang` and `text` |
| `table` | `header` and `rows` as a markdown table |
| `structure_table` | the Structure table, from the `structure` entries that have `table` |

Blocks are separated by one blank line and every file ends with one newline.

## Adding a module

Write the module in its folder (for example `agents/skills/<name>/SKILL.md`) and run `build`: each adapter links it on its own (`.claude/skills/<name>` -> `../../agents/skills/<name>`), never a whole folder. Links of deleted modules are removed on the next build.

## Removing a path

Deleting an entry from `config.json` does not delete it from disk. `build --check` reports leftover `.md` files in the root and `agents/`; delete them once you have confirmed they are no longer needed.
