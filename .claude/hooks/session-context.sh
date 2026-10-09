#!/usr/bin/env sh
# Print the working context at session start, resume and after compaction,
# so the architect continues from the sprint record instead of from memory.
# Never fails: a hook error must not block the session.
cd "${CLAUDE_PROJECT_DIR:-.}" 2>/dev/null || exit 0
branch=$(git branch --show-current 2>/dev/null || echo "?")
echo "Rivqen context: branch=${branch}"

# A sprint record is open until its State row says Closed (bold or not).
count=0
for f in docs/engineering/plan/sprints/*-S*.md; do
  [ -f "$f" ] || continue
  if grep -q -i -E '^\|[[:space:]]*State[[:space:]]*\|[[:space:]]*\**[[:space:]]*Closed' "$f"; then
    continue
  fi
  count=$((count + 1))
  [ "$count" -eq 1 ] && echo "Open sprint record(s) — read before you continue:"
  if [ "$count" -le 5 ]; then
    state=$(grep -m1 -i -E '^\|[[:space:]]*State[[:space:]]*\|' "$f" | sed 's/^|[^|]*|[[:space:]]*//; s/[[:space:]]*|[[:space:]]*$//')
    echo "  - $f (State: ${state:-missing})"
  fi
done
[ "$count" -gt 5 ] && echo "  … and $((count - 5)) more open record(s) in docs/engineering/plan/sprints/"
[ "$count" -eq 0 ] && echo "No open sprint. Next step comes from docs/engineering/plan/work-packages.md."

dirty=$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')
[ "$dirty" != "0" ] && echo "Uncommitted changes: ${dirty} file(s)."
exit 0
