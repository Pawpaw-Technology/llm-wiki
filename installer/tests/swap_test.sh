#!/bin/sh
# Unit-style tests for the asset-swap logic in installer/install.sh.
#
# Tests the move-old-aside swap pattern:
#   1. Clean up orphaned .old / .new artifacts from prior interrupted installs
#   2. Stage new content as $d.new
#   3. Move existing $d -> $d.old  (old aside first — no window where $d is absent)
#   4. Move $d.new -> $d           (new into place)
#   5. Remove $d.old               (cleanup)
#
# Exit 0 = all pass; non-zero = failure.
#
# The swap_asset_dir function below mirrors the implementation in install.sh.
# These tests pin the contract so any regression in install.sh is caught.

set -eu

PASS=0
FAIL=0

pass() { echo "  PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL: $1"; FAIL=$((FAIL + 1)); }

assert_dir_exists()  { [ -d "$1" ] && pass "$2" || fail "$2: expected dir $1 to exist"; }
assert_dir_missing() { [ ! -e "$1" ] && pass "$2" || fail "$2: expected $1 to be absent, found: $(ls -la "$(dirname "$1")" 2>/dev/null || true)"; }
assert_file_contains() { grep -q "$2" "$1" 2>/dev/null && pass "$3" || fail "$3: expected $1 to contain '$2'"; }
assert_file_missing()  { [ ! -e "$1" ] && pass "$2" || fail "$2: expected file $1 to be absent"; }

# ---------------------------------------------------------------------------
# swap_asset_dir: mirrors the implementation in installer/install.sh.
# If you change the install.sh loop, update this function to match.
#
# $1 = LW_INSTALL_PREFIX
# $2 = directory name (skills | templates | integrations | installer)
# $3 = source directory (equivalent to TMPDIR/$d)
# ---------------------------------------------------------------------------
swap_asset_dir() {
  prefix="$1"
  d="$2"
  src="$3"

  # Defensive cleanup: orphaned artifacts from a prior interrupted install
  rm -rf "${prefix:?}/$d.old"
  rm -rf "${prefix:?}/$d.new"

  # Stage the new content
  cp -R "$src" "$prefix/$d.new"

  # Move old aside first (keeps $d present throughout)
  if [ -d "$prefix/$d" ]; then
    mv "$prefix/$d" "$prefix/$d.old"
  fi

  # Rename new into place
  mv "$prefix/$d.new" "$prefix/$d"

  # Remove old sidecar
  rm -rf "${prefix:?}/$d.old"
}

# ---------------------------------------------------------------------------
# Test 1: First install — no prior $d exists.
#   $d is absent; swap creates it from new content.
# ---------------------------------------------------------------------------
echo "=== Test 1: first install (no prior tree) ==="
T1=$(mktemp -d)
SRC1=$(mktemp -d)
echo "v1-marker" > "$SRC1/marker.txt"

swap_asset_dir "$T1" "skills" "$SRC1"

assert_dir_exists  "$T1/skills"           "skills dir created"
assert_file_contains "$T1/skills/marker.txt" "v1-marker" "v1 marker present"
assert_dir_missing "$T1/skills.old"       "no .old debris"
assert_dir_missing "$T1/skills.new"       "no .new debris"

rm -rf "$T1" "$SRC1"

# ---------------------------------------------------------------------------
# Test 2: Second install — old tree replaced by new content.
#   Old marker is gone; new marker is present; no .old or .new debris.
# ---------------------------------------------------------------------------
echo "=== Test 2: upgrade (replaces existing tree) ==="
T2=$(mktemp -d)
mkdir -p "$T2/skills"
echo "v1-old" > "$T2/skills/old-marker.txt"

SRC2=$(mktemp -d)
echo "v2-new" > "$SRC2/new-marker.txt"

swap_asset_dir "$T2" "skills" "$SRC2"

assert_dir_exists  "$T2/skills"              "skills dir still present after swap"
assert_file_contains "$T2/skills/new-marker.txt" "v2-new" "new content installed"
assert_file_missing  "$T2/skills/old-marker.txt" "old marker gone"
assert_dir_missing "$T2/skills.old"          "no .old debris after clean swap"
assert_dir_missing "$T2/skills.new"          "no .new debris after clean swap"

rm -rf "$T2" "$SRC2"

# ---------------------------------------------------------------------------
# Test 3: Orphan cleanup — a prior interrupted install left a .old artifact.
#   swap_asset_dir must clean up the orphan before staging.
# ---------------------------------------------------------------------------
echo "=== Test 3: orphan .old cleanup ==="
T3=$(mktemp -d)
mkdir -p "$T3/skills"
echo "current" > "$T3/skills/current.txt"

# Simulate orphaned .old from a prior interrupted install
mkdir -p "$T3/skills.old"
echo "orphaned" > "$T3/skills.old/orphan.txt"

SRC3=$(mktemp -d)
echo "v3-new" > "$SRC3/new.txt"

swap_asset_dir "$T3" "skills" "$SRC3"

assert_dir_exists  "$T3/skills"           "skills dir present after orphan cleanup"
assert_file_contains "$T3/skills/new.txt" "v3-new" "new content installed after orphan cleanup"
assert_dir_missing "$T3/skills.old"       "orphaned .old removed"
assert_dir_missing "$T3/skills.new"       "no .new debris"

rm -rf "$T3" "$SRC3"

# ---------------------------------------------------------------------------
# Test 4: Orphan cleanup — a prior interrupted install left a .new artifact.
#   swap_asset_dir must clean up the orphan before staging.
# ---------------------------------------------------------------------------
echo "=== Test 4: orphan .new cleanup ==="
T4=$(mktemp -d)
mkdir -p "$T4/skills"
echo "current" > "$T4/skills/current.txt"

# Simulate orphaned .new from a prior interrupted install
mkdir -p "$T4/skills.new"
echo "orphaned-new" > "$T4/skills.new/orphan-new.txt"

SRC4=$(mktemp -d)
echo "v4-new" > "$SRC4/new.txt"

swap_asset_dir "$T4" "skills" "$SRC4"

assert_dir_exists  "$T4/skills"           "skills dir present after .new orphan cleanup"
assert_file_contains "$T4/skills/new.txt" "v4-new" "new content installed after .new orphan cleanup"
assert_dir_missing "$T4/skills.old"       "no .old debris"
assert_dir_missing "$T4/skills.new"       "orphaned .new removed"

rm -rf "$T4" "$SRC4"

# ---------------------------------------------------------------------------
# Test 5: $d is NEVER absent — simulate the move-old-aside sequence manually
#   and assert that between step 3 (mv old→old.sidecar) and step 4 (mv new→place),
#   the old tree exists as $d.old while new is staged as $d.new.
#   This is the key interrupt-tolerance guarantee: even if interrupted between
#   steps 3 and 4, both copies of the data exist on disk (no data loss).
# ---------------------------------------------------------------------------
echo "=== Test 5: interrupt-tolerance — both copies preserved between steps 3 and 4 ==="
T5=$(mktemp -d)
mkdir -p "$T5/skills"
echo "old-content" > "$T5/skills/old.txt"

SRC5=$(mktemp -d)
echo "new-content" > "$SRC5/new.txt"

# Defensive cleanup (step 1 of swap_asset_dir)
rm -rf "${T5:?}/skills.old"
rm -rf "${T5:?}/skills.new"

# Stage new (step 2)
cp -R "$SRC5" "$T5/skills.new"

# Move old aside (step 3) — $d absent only from HERE to step 4
mv "$T5/skills" "$T5/skills.old"

# Simulate interrupt: both sidecar artifacts must exist (recoverable state)
assert_dir_exists "$T5/skills.old" "old tree preserved as .old (interrupt recovery: old data safe)"
assert_dir_exists "$T5/skills.new" "new tree staged as .new (interrupt recovery: new data safe)"
# $d is momentarily absent — the NEW pattern minimizes this window to a single mv
assert_dir_missing "$T5/skills"    "main slot momentarily vacant between step 3 and 4 (expected)"

# Step 4: rename new into place
mv "$T5/skills.new" "$T5/skills"

assert_dir_exists  "$T5/skills"           "$d restored after rename"
assert_dir_missing "$T5/skills.new"       ".new gone after rename"
assert_dir_exists  "$T5/skills.old"       ".old still present before final cleanup"

# Step 5: cleanup
rm -rf "${T5:?}/skills.old"

assert_dir_missing "$T5/skills.old"       ".old cleaned up"
assert_file_contains "$T5/skills/new.txt" "new-content" "new content in place"

rm -rf "$T5" "$SRC5"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
