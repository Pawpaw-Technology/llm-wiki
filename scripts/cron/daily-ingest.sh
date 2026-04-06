#!/usr/bin/env bash
# Process new files dropped into raw/inbox/
# Usage: crontab -e → 0 8 * * * /path/to/daily-ingest.sh
set -euo pipefail
WIKI_ROOT="${WIKI_ROOT:?Set WIKI_ROOT to your wiki directory}"
cd "$WIKI_ROOT"

mkdir -p raw/inbox
count=0
for f in raw/inbox/*; do
  [ -f "$f" ] || continue
  lw ingest "$f" --category _uncategorized --yes
  count=$((count + 1))
done

if [ "$count" -gt 0 ]; then
  echo "Ingested $count file(s). Run 'lw status' to review."
else
  echo "No new files in raw/inbox/."
fi
