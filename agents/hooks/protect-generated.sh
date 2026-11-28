#!/bin/bash
# Claude Code PreToolUse hook (config.hooks.claude): denies Edit/Write on files that pla-build
# generates from the brain (memory.json, params.yaml). The decision is made by `pla-build --guard`, which reads the
# hook payload on stdin. If pla-build is not built yet, it warns and lets the edit through.
root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"
bin="$root/target/release/pla-build"
if [ ! -x "$bin" ]; then
    echo "protect-generated: $bin not built (cargo build --release); not guarding" >&2
    exit 1
fi
exec "$bin" --guard
