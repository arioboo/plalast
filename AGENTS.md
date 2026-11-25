# plalast - The Living Creature

This repo is a public and living creature. Turn on your VPS or machine, connect your agents, code it, teach it something, and make a pull request on Github.

The servers will be 24/7 on. This is a personal project on autonomous coding and living information systems, but everyone is welcome in.


## Best practices
- Zettelkasten markdown spirit. A lot of things in the web are garbage, so notes in the agent-zettelkasten use CDU (Universal Decimal Classification) notation: the file name starts with the CDU code, then a kebab-case title, e.g. `004.8-llm-agents.md` (004.8 = artificial intelligence). The infrastructure will be up.
- Specific formats: markdown for information, json for configs, bash for system scripts (setup, links, servers) and Rust for everything else (see Tools and languages).
- No duplication. Only one way of functional working, only one config file, symbolic links where matters, only one zettelkasten vault, etc.
- Using languages LLM-friendly like Rust, Python or Go is desirable. Also, using LLM programming-specific languages is a tool to proceed with caution, but desirable at some point. 
- Formal verification is desirable, but very expensive. Watch out and connect to others' ideas instead of reinventing the wheel
- Open source: everyone can collaborate with it (see README.md for the contribution flow)

## Structure
Paths are added here as they are created (`config.json` and `src/` are planned, not created yet):

| Path | Content |
|---|---|
| `zettelkasten` | Symlink to the single zettelkasten vault (CDU-named notes), outside this repo |
| `config.json` | The single config file |
| `src/` | Rust code |

## Commands
Not applicable yet: there is no code. Once each part exists:
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
