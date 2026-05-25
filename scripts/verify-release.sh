#!/usr/bin/env bash
set -euo pipefail

# verify-release.sh
#
# Mandatory local pre-tag / pre-push verification gate.
# Run this from the repository root *before* creating or pushing any vX.Y.Z tag
# (or release-related branch that will be tagged).
#
# It enforces:
#   - The exact reinforced gates required by project rules:
#       cargo fmt --all -- --check
#       cargo clippy -- -D warnings
#       cargo clippy --features llama-embed -- -D warnings
#       cargo test --all
#   - Extraction of version from the *local* Cargo.toml manifest (authoritative
#     source for cargo-dist; uses cargo pkgid with grep fallback).
#   - `cargo dist plan` succeeding for that version (prevents the "This workspace
#     doesn't have anything for dist to Release!" failure mode when manifest
#     version at tag does not align with intended release).
#
# On any failure: exits non-zero with actionable message (e.g. bump manifest first).
# On full success: prints confirmation banner naming the version and that it is
# safe to create the annotated tag and push.
#
# Usage:
#   ./scripts/verify-release.sh
#
# Requirements (normal Rust dev env):
#   - cargo, rustfmt, clippy, cargo-test
#   - cargo-dist installed (for the dist plan step)
#   - git (to locate repo root)
#
# This script is self-contained and intentionally minimal. Review before trusting
# in release flows. It performs no network, no mutations to user data, and no
# git operations beyond read-only toplevel detection.

SCRIPT_NAME=$(basename "$0")

usage() {
  echo "$SCRIPT_NAME - Mandatory pre-tag verification for release hygiene"
  echo ""
  echo "Usage: $0"
  echo ""
  echo "Must be invoked from the repository root (or a subdirectory; it will cd to toplevel)."
  echo "Run this *before* 'git tag -a vX.Y.Z' or pushing any tag that cargo-dist will act on."
  echo ""
  echo "The script will:"
  echo "  - Parse the version declared in the local Cargo.toml"
  echo "  - Run the full reinforced gate suite (fmt check + 2x clippy -D + test --all)"
  echo "  - Run 'cargo dist plan' and require exit 0 (verifies manifest version produces artifacts)"
  echo ""
  echo "If everything passes, it prints a success message with the version and safe-to-tag guidance."
  echo "Paste the full output of this script into the release commit or tag message."
  exit 0
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
fi

# Locate repo root (portable, works even if run from subdir)
if ! REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null); then
  echo "Error: not inside a git repository (or git not in PATH)." >&2
  echo "Run from within the checkout that contains Cargo.toml." >&2
  exit 1
fi
cd "$REPO_ROOT"

if [[ ! -f Cargo.toml ]]; then
  echo "Error: Cargo.toml not found at $REPO_ROOT" >&2
  echo "This script must be run from the project root." >&2
  exit 1
fi

echo "=== Pre-tag verification starting in $REPO_ROOT ==="
echo ""

# Robust version extraction from local manifest (never from tag arg or git describe).
# Primary: cargo pkgid (authoritative, includes the version cargo sees).
# Fallback: grep for ^version = "..." (handles simple Cargo.toml). Primary cargo pkgid path is authoritative; this assumes standard double-quoted [package] version (Cargo.toml convention) and takes the first match via head -1.
PKGID_OUTPUT=$(cargo pkgid 2>/dev/null || true)
if [[ -n "$PKGID_OUTPUT" && "$PKGID_OUTPUT" == *"@"* ]]; then
  VERSION="${PKGID_OUTPUT##*@}"
else
  VERSION=$(grep -E '^[[:space:]]*version[[:space:]]*=' Cargo.toml | head -1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
fi

if [[ -z "$VERSION" || ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "Error: could not extract a valid semver from local Cargo.toml (got: '${VERSION:-<none>}')" >&2
  echo "Check that the [package] version field is present and well-formed." >&2
  exit 1
fi

TAG="v$VERSION"
echo "Local Cargo.toml version : $VERSION"
echo "Derived tag name          : $TAG"
echo ""

# 1-4. Exact reinforced gates (quoted in project rules; must be clean before any tag).
echo "=== Step 1/5: cargo fmt --all -- --check ==="
if ! cargo fmt --all -- --check; then
  echo "" >&2
  echo "FAIL: cargo fmt --all -- --check reported differences." >&2
  echo "Action: run 'cargo fmt --all' (or cargo fmt --all -- --check to re-verify), then re-run $SCRIPT_NAME." >&2
  exit 1
fi
echo "PASS (clean)"

echo ""
echo "=== Step 2/5: cargo clippy -- -D warnings ==="
if ! cargo clippy -- -D warnings; then
  echo "" >&2
  echo "FAIL: cargo clippy (default features) exited non-zero with -D warnings." >&2
  echo "Fix the warnings, then re-run $SCRIPT_NAME." >&2
  exit 1
fi
echo "PASS (clean)"

echo ""
echo "=== Step 3/5: cargo clippy --features llama-embed -- -D warnings ==="
if ! cargo clippy --features llama-embed -- -D warnings; then
  echo "" >&2
  echo "FAIL: cargo clippy --features llama-embed exited non-zero with -D warnings." >&2
  echo "Fix the warnings (note: feature enables extra code paths), then re-run $SCRIPT_NAME." >&2
  exit 1
fi
echo "PASS (clean)"

echo ""
echo "=== Step 4/5: cargo test --all ==="
if ! cargo test --all; then
  echo "" >&2
  echo "FAIL: cargo test --all reported failures." >&2
  echo "Fix tests, then re-run $SCRIPT_NAME." >&2
  exit 1
fi
echo "PASS (all tests ok)"

echo ""
# 5. The critical dist plan (enforces manifest version has release artifacts configured).
echo "=== Step 5/5: cargo dist plan (must succeed for local manifest version $VERSION) ==="
if ! cargo dist plan; then
  echo "" >&2
  echo "FAIL: 'cargo dist plan' did not exit 0 for version $VERSION from local Cargo.toml." >&2
  echo "" >&2
  echo "Typical cause: the version declared in Cargo.toml has no matching release configuration," >&2
  echo "or the commit being tagged does not contain the intended version bump." >&2
  echo "" >&2
  echo "Example actionable message (per project rules):" >&2
  echo "  Version in Cargo.toml ($VERSION) has no dist artifacts configured or does not match intended tag." >&2
  echo "  Bump Cargo.toml first, then re-run." >&2
  echo "" >&2
  echo "After fixing the manifest version, commit the bump, re-run this script, then create the tag." >&2
  exit 1
fi
echo "PASS (cargo dist plan succeeded for v$VERSION)"

# Optional json confirmation if jq is available (nice-to-have, never required).
if command -v jq >/dev/null 2>&1; then
  if cargo dist plan --output-format=json 2>/dev/null | jq -e --arg ver "$VERSION" '
      (.releases // []) | any(.version == $ver or .app_version == $ver)
    ' >/dev/null 2>&1; then
    echo "     (jq: version $VERSION explicitly present in dist plan JSON)"
  else
    echo "     (jq present but version not matched in JSON; plain plan succeeded so continuing)"
  fi
fi

echo ""
echo "================================================================"
echo "All gates passed for v$VERSION from local Cargo.toml manifest."
echo "Safe to create annotated tag $TAG and push."
echo ""
echo "Recommended next commands (after reviewing the above output):"
printf "  git tag -a %s -m 'Release %s'\n" "$TAG" "$TAG"
printf "  git push origin main --tags\n"
echo "================================================================"

exit 0
