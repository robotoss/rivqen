#!/usr/bin/env sh
# Print the working context at session start and after compaction,
# so the architect continues from the sprint record instead of from memory.
# Never fails: a hook error must not block the session.
cd "${CLAUDE_PROJECT_DIR:-.}" 2>/dev/null || exit 0
branch=$(git branch --show-current 2>/dev/null || echo "?")
echo "Rivqen context: branch=${branch}"
# Every sprint record whose State row is not "Closed" is open work.
open_records=$(grep -L -E '^\| State \| *Closed' docs/engineering/plan/sprints/*-S*.md 2>/dev/null | head -n 5)
if [ -n "$open_records" ]; then
  echo "Open sprint record(s) — read before you continue:"
  for f in $open_records; do
    state=$(grep -m1 -E '^\| State \|' "$f" | sed 's/^| State | *//; s/ *|$//')
    echo "  - $f (State: ${state:-unknown})"
  done
else
  echo "No open sprint. Next step comes from docs/engineering/plan/work-packages.md."
fi
dirty=$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')
[ "$dirty" != "0" ] && echo "Uncommitted changes: ${dirty} file(s)."
exit 0
