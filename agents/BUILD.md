# BUILD
note for agent: ...

Single source: every `.md` in the repo root and in `agents/` is generated from `memory.json` and `params.yaml`: change `memory.json` (and `params.yaml` only as Params untouchable allows), then run `cargo run --release --bin pla-build` (see `agents/BUILD.md`); never edit a generated file by hand. Hand-written: the Rust code, the scripts and the modules in `agents/` subfolders.

## Commands

| Command | What it does |
|---|---|
| `cargo run --release --bin pla-build` | Builds the repo from `memory.json` and `params.yaml`. Idempotent: only writes what changed. |
| `cargo run --release --bin pla-build -- --check` | Changes nothing; exits non-zero if the repo drifted from the memory (stale file, missing link, stray `.md`). |
| `cargo run --release --bin pla-build -- --guard` | Reads a Claude Code PreToolUse payload on stdin and denies Edit/Write/NotebookEdit on any file `pla-build` generates and on any path outside the repo root; `agents/hooks/protect-generated.sh` runs it. |
| `cargo run --release --bin pla-recall -- [term]` | Without `term`, prints the map: every path with its type and description, the module kinds, features, log, index, settings and levers. With `term`, searches all of it and the document text (case-insensitive), naming the file, section and lever of each hit. |
| `cargo run --release --bin pla-lint -- [memory] [modules] [rust]` | Checks that `memory.json` and `params.yaml` are consistent and fully documented in `primary.schema`, that hand-written modules have the right shape (skill folders with `SKILL.md`, frontmatter `name` and `description`), that guides have their source, that the texts of the memory say each thing once and point at paths, dotted keys and binaries that exist, and that `cargo fmt` and `cargo clippy` pass (formal verification is in `pla-verify`). Pass `memory`, `modules` or `rust` to run only some areas. |
| `cargo run --release --bin pla-audit -- [secrets] [deps] [repo]` | Looks for keys, tokens, emails and home-folder paths in what git would commit, files committed inside secret folders, known vulnerabilities in `Cargo.lock` (with `cargo-audit`), broken links, large files, files with the same content, uncommitted changes and branches out of sync. Pass `secrets`, `deps` or `repo` to run only some areas; `config.audit` holds its settings. |
| `cargo run --release --bin pla-verify -- [consistency] [logic] [formal]` | Runs the verification levels of a change and reports each one: `consistency` (`pla-build --check` and `pla-lint memory modules`), `logic` (`pla-lint rust`, then `cargo test`, including the logic tests of `tests/logic.rs`) and `formal` (`cargo verus verify`; skipped with a warning when Verus is not installed). It ends by listing the changed files for the human review. Pass `consistency`, `logic` or `formal` to run only some levels. |
| `cargo run --release --bin pla-calc -- <expression>` | Evaluates an arithmetic expression with `plalast::calc`: exact fractions (`0.1 + 0.2` is `3/10`), `+ - * /`, `^` with an integer exponent, unary minus and parentheses. Prints the exact value and, for a fraction, its decimal digits (marked `…` when cut). Exit 1 when the expression has no value (division by zero, overflow, a syntax error), 2 without an expression. |
| `git config core.hooksPath agents/hooks` | Run once per clone: git then uses `agents/hooks`, whose `pre-commit` refuses a commit while `pla-build --check` fails. `pla-audit` warns when it is not set. |
| `cargo install cargo-audit --locked` | Run once per machine (it compiles from source and can take about 15 minutes): `pla-audit deps` then checks `Cargo.lock` against the RustSec advisory database. Without it, that check is skipped with a warning. |

Run them from anywhere inside the repo: the tool walks up to the folder that holds `memory.json`. Symlinks need a Unix system (Linux or macOS). `pla-build` never replaces a real file or folder with a link, and never deletes anything except broken module links.

## memory.json and params.yaml

The brain is `memory.json`, with exactly three primary keys: `config` (what the tools need to run: git, settings, paths), `primary` (what makes the docs: identity, texts, definitions) and `data` (what gets built); each starts with `descr_+` and `descr_-`, what it is and is not. `params.yaml` holds what only the user sets, every key named by the path of a field of the memory: a value `enabled`, `on_demand` or `disabled` is the interruptor of that field (after `#---INTERRUPTORS---`); any other value sets the field (such as `config.hooks.dir`), which then must not be in `memory.json` too. The tools join them as `config`, `primary`, `data` and `params` (`params.interruptors`), so a dotted path such as `config.paths.modules.agent_zettelkasten` works everywhere.

| Key | Is (descr_+) | Is not (descr_-) |
|---|---|---|
| `config.descr_+` | What `config` is (the positive way), in the essential 20% that carries 80% of the meaning (Pareto rule). | Not a list of contents: one sentence, the essential 20%. |
| `config.descr_-` | What `config` is not (the negative way), in the essential 20% that carries 80% of the meaning (Pareto rule). | Not a repeat of `descr_+` in negative form: what a reader could wrongly put there. |
| `config.PLALAST_DIR` | Where the repo lives: always `$PWD`, the repo root, which the tools find by walking up to `memory.json` and resolve at run time. No path is written, so the same value works on every machine and never leaks the user name. | Not a written path: a written one works on one machine only and leaks the user name. |
| `config.repository` | Remote repository of the project. | Not a clone URL with credentials, not the branches (`config.branches`). |
| `config.branches` | `integration` (where work is committed) and `stable` (what receives merges); `pla-audit` checks they are in sync. Set in `params.yaml`. | Not the list of all branches: only the two the flow depends on. |
| `config.audit` | Settings of `pla-audit`: `allow` (exact strings or repo paths that are never reported, such as a public contact email) and `max_file_kb` (larger files are reported). (`allow` is set in `params.yaml`) | Not the audit itself (`pla-audit`), not the lint settings (`config.lint`). |
| `config.lint` | Settings of `pla-lint`: `always_on` (docs an agent loads every session) and `max_lines` (their combined line budget; over it, move sections to `on_demand`). (`max_lines` is set in `params.yaml`) | Not the lint itself (`pla-lint`), not which sections are always on (the interruptores). |
| `config.hooks` | Hooks that enforce the rules: `dir` (scripts; also git's `core.hooksPath`, so `pre-commit` runs `pla-build --check`) and `claude` (Claude Code hooks, rendered into `agents/hooks/claude-settings.json`, which `.claude/settings.json` links to; the PreToolUse hook denies hand edits of generated files and any write outside the repo root). Set in `params.yaml`. | Not the hook scripts (they live in `agents/hooks/`), not git settings of the machine. |
| `config.paths` | Every path the tools use: `docs`, `on_demand` and `modules`. | Not file contents, not the structure of the repo (`config.structure`). |
| `config.paths.docs` | Folders whose `.md` files are docs (`.` is the repo root). Every `.md` directly inside them must be generated. | Not every folder with markdown: guides and modules are hand-written, not docs. |
| `config.paths.on_demand` | Folder where `on_demand` sections are written as `<id>.md` and `on_demand` modes as `mode-<name>.md`. | Not where hand-written modules go (`config.paths.modules`). |
| `config.paths.modules` | Per module kind (skills, agents, rules, connectors, guides): `dir` (where its modules are written) and `links` (provider -> folder that gets one symlink per module). Per external module of `primary.modules` (such as `agent_zettelkasten`): its path, relative to `modules/` (or to the folder of a structure symlink whose `target_ref` points at it). Keep it relative: an absolute path leaks your home folder. It depends on where the repo and the module sit on the machine, so a machine with another layout changes the value. | Not the modules themselves, not their definitions (`primary.<kind>`, `primary.modules`). |
| `config.structure` | Every path of the repo, in order, as the tools need it: `path` (or `path_ref`), `type`, and optional `build`, `gitkeep`, `secret`, `scratch` (contents git-ignored except `.gitkeep`, for the agent's scratch), `links_only` (a folder of symlinks into `agents/` only), `target` (or `target_ref`) and `local`. Its prose is in `primary.structure`. | Not prose: descriptions and table texts are in `primary.structure`, path by path. |
| `config.obligatory` | The categories of AGENTS.md that must always be followed, as memory paths (`primary.agents-md.<category>`); their sections are titled `(obligatory)`. Set in `params.yaml`. | Not a list of single rules, and never a category that is also preferential. |
| `config.preferential` | The categories of AGENTS.md that are followed when they fit, as memory paths; their sections are titled `(preferential)`. Set in `params.yaml`. | Not optional to read: only to apply, when another rule does not decide. |
| `primary.descr_+` | What `primary` is (the positive way), in the essential 20% that carries 80% of the meaning (Pareto rule). | Not a list of contents: one sentence, the essential 20%. |
| `primary.descr_-` | What `primary` is not (the negative way), in the essential 20% that carries 80% of the meaning (Pareto rule). | Not a repeat of `descr_+` in negative form: what a reader could wrongly put there. |
| `primary.project` | Identity: `name`, `title` and `plalast_logo` (path of the project logo). | Not the repository settings (`config.repository`), not the README text. |
| `primary.docs` | Everything about the docs, the markdown files about this project and how it operates. | Not the text of any doc: only what is common to all of them. |
| `primary.docs.definition` | What counts as a doc, in words. | Not a list of the docs (that is `config.structure`). |
| `primary.docs.on_demand` | How `on_demand` sections are pointed at: the heading of the pointer list (`heading`) and the pointer line (`pointer`, with `{path}`, `{title}` and `{when}`). Their folder is `config.paths.on_demand`. | Not the `on_demand` files, not their folder (`config.paths.on_demand`). |
| `primary.docs.content` | One document per key: `head` (blocks before the first section), `sections` (`id`, `title`, `when`, `blocks`) and optional `agent_note: false` (no agent note, for adapter files such as `CLAUDE.md`, which is only `@AGENTS.md`). A `file` entry with `build: "<key>"` is rendered from it. A whole doc can be switched with the interruptor `primary.docs.content.<key>` (`disabled`: not built). | Not prose: blocks point at texts in `primary` with `ref`. |
| `primary.docs.agent_placeholder` | The note on the second line of every generated doc (the `agent_note` block, and every `on_demand` file), such as `note for agent: ...`. Set in `params.yaml`. | Not a rule and not the title: only the second line of each doc. |
| `primary.agents-md` | The AGENTS.md text, one key per category of the doc: `intro` (under the title), `static_rules`, `static_premises`, `dynamic_premises`, `good_practices` and `preferences`; `primary.docs.content.agents_md` gives the doc its shape and points at them with `ref`. | Not the shape of AGENTS.md (`primary.docs.content.agents_md`), not the doc itself. |
| `primary.agents-md.intro` | Paragraphs under the title. | Not a rule: no premise, practice or preference goes here. |
| `primary.agents-md.static_rules` | The Reglas - estáticas section: `items`, the rules that never bend and that the tools enforce (generation from the brain, writes only inside the root, params only by the user, secrets). | Not premises to reason from (static premises), not things still to verify (dynamic premises). |
| `primary.agents-md.static_premises` | The Premisas - estáticas section: `items` (the fixed premises) and `brain_keys_intro` (intro of the table of primary keys, the `brain_keys` block). | Not things still to verify (those are dynamic), not how to work (good practices). |
| `primary.agents-md.dynamic_premises` | The Premisas - dinámicas section: `intro` (dynamic premises are still to be verified, each the current solution to a problem), `items` (the premises), then the current state: `state_intro`, `map`, `modes_intro` and the `modes` block, `commands` (`tool_intro`, `code_intro`, `code`; the tool commands come from `primary.commands`), `tools_intro` and `tools_and_languages`, `candidates_intro` and `candidates`. | Not fixed truths: a dynamic premise may change once a better solution is verified. |
| `primary.agents-md.good_practices` | The Buenas prácticas section: `items`, how every agent works in the repo. | Not premises (what the repo is), not tastes of the owner (preferences). |
| `primary.agents-md.preferences` | The Preferencias section: `items`, preferences of the project owner. | Not rules that must never bend: those are premises or checks. |
| `primary.features` | Features of the project, as `feature_<n>` -> `{descr, status}` (both strings). | Not a roadmap of ideas, not the log of what was done (`primary.log`). |
| `primary.log` | Project log: entries `{date, branch, commit, kind, descr}` in the order they happened; `kind` is `commit`, `merge` or `branch`. A commit cannot hold its own hash, so each one is logged in the next commit. LOG.md shows them newest first; `pla-audit` warns about commits missing from it. | Not a changelog of ideas or plans: only branches, commits and merges that happened. |
| `primary.index` | Project index (empty for now; its format is not defined yet). | Not the map of paths (`config.structure`, `agents/structure.md`). |
| `primary.skills` | Skills: `definition`; its folder and provider links are in `config.paths.modules`. | Not the skills themselves (they are hand-written in their folder). |
| `primary.agents` | Subagents: `definition`; its folder and provider links are in `config.paths.modules`. | Not the subagents themselves (they are hand-written in their folder). |
| `primary.rules` | Rules: `definition`; its folder and provider links are in `config.paths.modules`. | Not the rules of AGENTS.md (premises, practices, preferences): these are hand-written modules. |
| `primary.connectors` | MCP connectors: `definition`; its folder and provider links are in `config.paths.modules`. `pla-lint` checks each file's neutral format. | Not the connectors themselves, and never their secrets (those live in `.secrets/`). |
| `primary.guides` | Guides: `definition` and `render` (`pdf`: each .md is rendered to a .pdf by `pla-build`); folder and links in `config.paths.modules.guides`. | Not the guides themselves (hand-written `.md` in their folder), not the generated PDFs. |
| `primary.modules` | External modules that live outside this repo, by name (such as `agent_zettelkasten`): each has a `definition` (what it is, in words); its path is `config.paths.modules.<name>`. A symlink in `modules/` is built only if `config.structure` has one for it. | Not module kinds of this repo (`primary.skills` and the like): only what lives outside it. |
| `primary.mode` | Modes, by name (such as `ultra_conservative`): `title`, `when` and `rules` (a list of strings). Its interruptor is `primary.mode.<name>` in `params.yaml`: `enabled` writes the rules into the `modes` block, `on_demand` writes them to `<config.paths.on_demand>/mode-<name>.md` and leaves a pointer, `disabled` drops them. | Not the interruptor of a mode (that is in `params.yaml`), not a doc section. |
| `primary.procedures` | Procedures: `definition` and `items` (by id: `title` and `text`, a list of paragraphs in human language, with no backticks or file names; `pla-lint` checks it). Rendered into `agents/PROCEDURES.md` by the `procedures` block. | Not rules (premises, practices), not technical steps with commands or paths. |
| `primary.problems` | Problems: `definition` and `items` (by id: `title`, `statement` in human language with no backticks or file names, and `solved_by`, the names of the rules that solve it). A rule's name is its text up to the first `:` or `. `. Rendered into `agents/PROBLEMS.md` by the `problems` block; `pla-lint` checks every name resolves to exactly one rule and every rule solves a problem. | Not solutions: the rules that solve a problem live in `primary.agents-md` and are only named here. |
| `primary.structure` | The prose of every path, by path: `description`, and optional `table` and `label` (its row in the Structure table, with `{path}`). `pla-lint` pairs it with `config.structure`, path by path. | Not the path settings (types, flags, how it is built): those are in `config.structure`. |
| `primary.schema` | This documentation of the memory and params format; `BUILD.md` renders its tables. | Not the data it describes, not a copy of the docs. |
| `primary.commands` | The repo's commands, each `{cmd, summary, detail}`: AGENTS.md lists the summaries and BUILD.md the details. | Not the code commands of AGENTS.md (`primary.agents-md.dynamic_premises.commands`): only the repo's own tools and setup. |
| `data.descr_+` | What `data` is (the positive way), in the essential 20% that carries 80% of the meaning (Pareto rule). | Not a list of contents: one sentence, the essential 20%. |
| `data.descr_-` | What `data` is not (the negative way), in the essential 20% that carries 80% of the meaning (Pareto rule). | Not a repeat of `descr_+` in negative form: what a reader could wrongly put there. |
| `params` | What `params.yaml` holds once loaded: the interruptors, by the path of the field they switch (`params.interruptors`). Its other params set fields of the memory at their path (such as `config.hooks`), so they appear there. | Not something an agent changes on its own (Params untouchable, Memory updatable). |
| `params.interruptors` | Interruptor (memory lever) of every doc section (`primary.docs.content.<doc>.sections.<id>`), whole doc (`primary.docs.content.<doc>`) and mode (`primary.mode.<name>`), by that path; written after `#---INTERRUPTORS---`. Unset means `on_demand`. | Not the sections, docs or modes themselves, only whether they are loaded. |

## Interruptors (params.yaml)

Every doc section (`primary.docs.content.<doc>.sections.<id>`), whole doc (`primary.docs.content.<doc>`) and mode (`primary.mode.<name>`) can have an interruptor in `params.yaml`, after `#---INTERRUPTORS---`. An unset one is `on_demand`; one for something that does not exist, or a value outside this table, stops the build or fails `pla-lint`.

| Value | Effect |
|---|---|
| `enabled` | Rendered inside its document as `## <title>`: always in the agent's memory. |
| `on_demand` | Rendered to `<config.paths.on_demand>/<id>.md`; the document keeps only a pointer line under the on_demand heading, and the agent reads the file when `when` applies. The default when no value is set. |
| `disabled` | Not rendered anywhere. |

Switching a section from `on_demand` to `disabled` leaves its old file behind: `pla-build` reports it as a stray `.md` and fails until you delete it.

## Structure entry types

| Type | Build action |
|---|---|
| `dir` | Creates the folder; `gitkeep: true` adds an empty `.gitkeep`; `secret: true` git-ignores its contents except `.gitkeep` (and `pla-audit` reports anything committed there); `scratch: true` does the same for the agent's own folder, without that report. `path_ref` (a dotted path such as `config.paths.modules.skills.dir`) can replace `path`, and `label` may use `{path}`. |
| `symlink` | Creates a link to `target`, relative to the link's folder; `target_ref` (a dotted config path such as `config.paths.modules.agent_zettelkasten`) can replace `target`. `local: true` marks a link that depends on the machine's layout: it is git-ignored, built only where its target exists, and removed where it does not. |
| `file + build` | Renders `primary.docs.content.<build>`; `build: "gitignore"` renders `.gitignore` from the secret, ignored and local entries; `build: "claude_settings"` renders `agents/hooks/claude-settings.json` from `config.hooks.claude`. |
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
| `table` | `header` and `rows` as a markdown table; or `ref` to a list of objects (or a mapping of objects, whose key fills `{key}`) with `row` (one template per column) and optional `reverse` |
| `structure_table` | the Structure table, from the `config.structure` entries that have `table` |
| `schema` | a table of `primary.schema.<part>` with the given `header`; a field of `keys` is `{descr_+, descr_-}`, shown as two columns |
| `image` | `![alt](src)`, or an HTML `<img>` when `width` is set; `ref` (a dotted config path such as `primary.project.plalast_logo`) can replace `src` |
| `agent_note` | the text of `primary.docs.agent_placeholder` (set in `params.yaml`), on the line right after the first one (no blank line); every doc has it as the second block of its `head`, unless the doc sets `agent_note: false` |
| `modes` | one `- Mode` line per mode of `primary.mode` that is not `disabled`: its rules when `enabled`, a pointer to `mode-<name>.md` when `on_demand`; nothing when every mode is disabled |
| `brain_keys` | a table `Key | Is (descr_+) | Is not (descr_-)` with one row per primary key of the memory (`config`, `primary`, `data`) |
| `procedures` | one `## <title>` per procedure of `primary.procedures.items`, followed by its paragraphs |
| `problems` | one `## <title>` per problem of `primary.problems.items`: its statement and a `Solved by:` line with the names of its rules |
| `always_read` | the intro of its `ref` and one `- path` line per generated doc whose interruptor is `enabled`; nothing when there are none |

Each section is rendered as `## <title>` followed by its blocks. Blocks are separated by one blank line, except `agent_note`, which goes right under the first line; every file ends with one newline.

## Adding a module

Write the module in its folder (for example `agents/skills/<name>/SKILL.md`) and run `pla-build`. Claude Code sees it through its folder links (`.claude/skills` -> `../agents/skills`). A provider that wants one link per module lists its folder in `config.paths.modules.<kind>.links`; `pla-build` then links each module there and removes the links of deleted modules.

Guides are modules too: write `agents/guides/<name>.md` and run `pla-build`, which renders `agents/guides/<name>.pdf` from it (headings, paragraphs, lists and code blocks; inline markup is flattened). The PDF is deterministic, so `pla-build --check` reports a stale one.

## Removing a path

Deleting an entry from `memory.json` does not delete it from disk. `pla-build --check` reports leftover `.md` files in the root and `agents/`; delete them once you have confirmed they are no longer needed.

## On demand

- `agents/structure.md`: Structure. Read it when looking for where something lives or adding a path.
