#!/bin/sh
# Unit-style tests for the integrate-gate logic in installer/install.sh.
#
# Asserts that auto-integration is only triggered when LW_INSTALL_PREFIX
# equals the default prefix ($HOME/.llm-wiki). Under a custom prefix the
# integrate block must be suppressed so uninstall.sh's invariant holds true:
# "custom-prefix installs never touched user agent configs."
#
# Strategy: place a stub `lw` binary on PATH that writes to a marker file
# when invoked with `integrate`. Then source the gate condition logic
# (replicated from install.sh) and assert the marker exists/absent.
#
# Exit 0 = all pass; non-zero = failure.

set -eu

PASS=0
FAIL=0

pass() { echo "  PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL: $1"; FAIL=$((FAIL + 1)); }

assert_file_exists()  { [ -f "$1" ] && pass "$2" || fail "$2: expected file $1 to exist"; }
assert_file_missing() { [ ! -e "$1" ] && pass "$2" || fail "$2: expected file $1 to be absent"; }

# ---------------------------------------------------------------------------
# should_auto_integrate: mirrors the gate condition from install.sh.
# Returns 0 (true) if auto-integrate should run, 1 (false) if it should be
# suppressed.
#
# $1 = LW_NO_INTEGRATE (0 or 1)
# $2 = LW_YES (0 or 1)
# $3 = LW_INSTALL_PREFIX (actual prefix in use)
# $4 = default_prefix ($HOME/.llm-wiki)
# ---------------------------------------------------------------------------
should_auto_integrate() {
  no_integrate="$1"
  lw_yes="$2"
  install_prefix="$3"
  def_prefix="$4"

  if [ "$no_integrate" -eq 1 ]; then
    return 1  # --no-integrate suppresses everything
  elif [ "$lw_yes" -eq 1 ] && [ "$install_prefix" = "$def_prefix" ]; then
    return 0  # default prefix + --yes => auto-integrate
  else
    return 1  # custom prefix or no --yes => suppress
  fi
}

# ---------------------------------------------------------------------------
# run_gate: exercises the full integrate branch logic from install.sh using
# a stub lw binary that records invocations.
#
# $1 = LW_NO_INTEGRATE
# $2 = LW_YES
# $3 = LW_INSTALL_PREFIX
# $4 = default_prefix
# $5 = marker file path (written by stub lw when integrate is called)
# ---------------------------------------------------------------------------
run_gate() {
  no_integrate="$1"
  lw_yes="$2"
  install_prefix="$3"
  def_prefix="$4"
  marker="$5"

  if [ "$no_integrate" -eq 1 ]; then
    :  # suppressed
  elif [ "$lw_yes" -eq 1 ] && [ "$install_prefix" = "$def_prefix" ]; then
    # default prefix + --yes: invoke integrate
    "$install_prefix/bin/lw" integrate --auto --yes || true
  elif [ "$lw_yes" -eq 1 ]; then
    # custom prefix + --yes: suppressed with informational message (no invocation)
    :
  fi
}

# ---------------------------------------------------------------------------
# Test setup: create a stub lw binary that writes to a marker file when
# invoked with `integrate` as the first argument.
# ---------------------------------------------------------------------------
make_stub_lw() {
  prefix="$1"
  marker="$2"
  mkdir -p "$prefix/bin"
  cat > "$prefix/bin/lw" <<STUB
#!/bin/sh
if [ "\$1" = "integrate" ]; then
  touch "$marker"
fi
exit 0
STUB
  chmod +x "$prefix/bin/lw"
}

# ---------------------------------------------------------------------------
# Test 1: Default prefix + LW_YES=1 => integrate IS invoked.
# ---------------------------------------------------------------------------
echo "=== Test 1: default prefix + LW_YES=1 => integrate invoked ==="
T1=$(mktemp -d)
MARKER1="$T1/integrate_called"
default_prefix="${HOME:-}/.llm-wiki"

make_stub_lw "$T1" "$MARKER1"

# Run gate logic with default prefix
run_gate 0 1 "$T1" "$T1" "$MARKER1"

assert_file_exists "$MARKER1" "integrate invoked when LW_INSTALL_PREFIX=default and LW_YES=1"

rm -rf "$T1"

# ---------------------------------------------------------------------------
# Test 2: Custom prefix + LW_YES=1 => integrate is NOT invoked.
# ---------------------------------------------------------------------------
echo "=== Test 2: custom prefix + LW_YES=1 => integrate suppressed ==="
T2=$(mktemp -d)
custom_prefix="$T2/custom"
MARKER2="$T2/integrate_called"
mkdir -p "$custom_prefix"

make_stub_lw "$custom_prefix" "$MARKER2"

# Run gate logic: install_prefix differs from def_prefix (simulate default as different path)
run_gate 0 1 "$custom_prefix" "$T2/default_that_doesnt_match" "$MARKER2"

assert_file_missing "$MARKER2" "integrate suppressed when LW_INSTALL_PREFIX=custom and LW_YES=1"

rm -rf "$T2"

# ---------------------------------------------------------------------------
# Test 3: LW_NO_INTEGRATE=1 => integrate is NOT invoked even with default prefix.
# ---------------------------------------------------------------------------
echo "=== Test 3: LW_NO_INTEGRATE=1 => integrate always suppressed ==="
T3=$(mktemp -d)
MARKER3="$T3/integrate_called"

make_stub_lw "$T3" "$MARKER3"

run_gate 1 1 "$T3" "$T3" "$MARKER3"

assert_file_missing "$MARKER3" "integrate suppressed when LW_NO_INTEGRATE=1"

rm -rf "$T3"

# ---------------------------------------------------------------------------
# Test 4: Default prefix + LW_YES=0 => integrate is NOT invoked (non-interactive,
# non-yes path — would fall through to TTY branch which we don't exercise here).
# ---------------------------------------------------------------------------
echo "=== Test 4: default prefix + LW_YES=0 => integrate not invoked ==="
T4=$(mktemp -d)
MARKER4="$T4/integrate_called"

make_stub_lw "$T4" "$MARKER4"

run_gate 0 0 "$T4" "$T4" "$MARKER4"

assert_file_missing "$MARKER4" "integrate not invoked when LW_YES=0"

rm -rf "$T4"

# ---------------------------------------------------------------------------
# Test 5: should_auto_integrate helper — unit test the gate function itself.
# ---------------------------------------------------------------------------
echo "=== Test 5: should_auto_integrate unit tests ==="

def_prefix="${HOME:-}/.llm-wiki"

# 5a: default prefix + yes => should integrate
if should_auto_integrate 0 1 "$def_prefix" "$def_prefix"; then
  pass "should_auto_integrate returns true for default prefix + yes"
else
  fail "should_auto_integrate returned false for default prefix + yes"
fi

# 5b: custom prefix + yes => should NOT integrate
if should_auto_integrate 0 1 "/tmp/custom-lw" "$def_prefix"; then
  fail "should_auto_integrate returned true for custom prefix + yes (BUG)"
else
  pass "should_auto_integrate returns false for custom prefix + yes"
fi

# 5c: no-integrate flag => should NOT integrate even with default prefix
if should_auto_integrate 1 1 "$def_prefix" "$def_prefix"; then
  fail "should_auto_integrate returned true with LW_NO_INTEGRATE=1 (BUG)"
else
  pass "should_auto_integrate returns false when LW_NO_INTEGRATE=1"
fi

# 5d: custom prefix + LW_YES=0 => should NOT integrate
if should_auto_integrate 0 0 "/tmp/custom-lw" "$def_prefix"; then
  fail "should_auto_integrate returned true when LW_YES=0 (BUG)"
else
  pass "should_auto_integrate returns false when LW_YES=0"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
