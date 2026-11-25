# plalast - The Living Creature

<img src="src/plalast.png" alt="plalast logo" width="200">

A public, living repository on autonomous coding and living information systems. Servers run 24/7; anyone can connect their agents, teach it something and open a pull request.

Project rules and conventions for humans and agents: [agents/AGENTS.md](agents/AGENTS.md). Everything is generated from [config.json](config.json); see [agents/BUILD.md](agents/BUILD.md).

## Contributing

1. Fork the repo and create a branch from `develop` (`feat/<topic>`, `fix/<topic>`, `docs/<topic>`).
2. Change only `config.json` (or a hand-written module in an `agents/` subfolder), run `cargo run --release --bin pla-build`, and keep each change small: no duplication, Rust by default, markdown/json/bash formats.
3. Run `pla-lint` and `pla-audit` (see *Commands* in agents/AGENTS.md): both must finish with no errors.
4. Open a pull request against `develop` describing what changed and why. `main` only receives merges from `develop`.
