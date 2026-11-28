#!/bin/bash
# Claude Code SessionStart hook (config.hooks.claude): checks the brain at run time and adds to
# the context the memory level and what can be recalled on demand (`pla-recall --session`).
root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel)}"
bin="$root/target/release/pla-recall"
if [ ! -x "$bin" ]; then
    echo "session-recall: $bin not built (cargo build --release); the brain was not checked" >&2
    exit 0
fi
cd "$root" && exec "$bin" --session
