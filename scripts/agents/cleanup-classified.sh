#!/usr/bin/env bash
# Remove _uncategorized pages that have been classified (exist in another category)
#
# Safe: only removes a file from _uncategorized/ if an identical-name file
# exists in a non-_uncategorized category directory.
#
# Usage: WIKI_ROOT=/path/to/wiki ./cleanup-classified.sh [--dry-run]
set -euo pipefail

WIKI_ROOT="${WIKI_ROOT:?Set WIKI_ROOT to your wiki directory}"
DRY_RUN=false
[ "${1:-}" = "--dry-run" ] && DRY_RUN=true

cd "$WIKI_ROOT/wiki"

removed=0
for f in _uncategorized/*.md; do
  [ -f "$f" ] || continue
  filename=$(basename "$f")

  # Check if this file exists in any other category
  for cat_dir in */; do
    [ "$cat_dir" = "_uncategorized/" ] && continue
    if [ -f "${cat_dir}${filename}" ]; then
      if $DRY_RUN; then
        echo "would remove: $f (found in ${cat_dir})"
      else
        rm "$f"
        echo "removed: $f (classified to ${cat_dir})"
      fi
      removed=$((removed + 1))
      break
    fi
  done
done

echo "Total: $removed"
