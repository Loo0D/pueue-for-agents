#!/usr/bin/env bash
# This project has special rules regarding the usage of LLMs.
# User confirmation is required to prevent the LLM from generating this comment block.
#
# Release pueue-for-agents to the Homebrew tap.
#
# The artifacts are compiled by .github/workflows/release.yml on GitHub-hosted
# native runners, because the darwin binaries cannot be cross-compiled from a
# Linux box (Rust links against the macOS SDK). This script owns everything
# after the compile: tag, wait, download, checksum, publish to Tigris, formula,
# tap push.
#
# Artifacts are named `pueue-for-agents_<version>_<os>_<arch>.tar.gz` and carry
# both binaries (`pueue` and `pueued`). The formula name is `pueue-for-agents`
# so it never collides with the homebrew-core `pueue` package, while the
# installed binaries keep their upstream names.
#
# Usage:
#   scripts/release.sh <version>
#   scripts/release.sh <version> --run-id <github-run-id>   # resume: reuse a CI run
#
# Examples:
#   scripts/release.sh 4.0.5-for-agents.1

set -euo pipefail

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
note() { printf '%s\n' "$*"; }

VERSION=""
RUN_ID=""
while [ $# -gt 0 ]; do
  case "$1" in
    --run-id)
      [ -n "${2:-}" ] || die "--run-id needs a value"
      RUN_ID="$2"
      shift 2
      ;;
    -h|--help)
      cat <<'USAGE'
Usage: scripts/release.sh <version> [--run-id <github-run-id>]

  <version>        must equal the version in pueue/Cargo.toml
  --run-id <id>    publish from an existing GitHub Actions run instead of
                   tagging and waiting for a fresh build

The artifacts are built by .github/workflows/release.yml on native runners
(linux amd64/arm64 musl, darwin amd64/arm64, windows amd64/arm64), published to
the t3://tap-ozoxoz bucket, and pinned in ~/homebrew-tap/pueue-for-agents.rb.
USAGE
      exit 0
      ;;
    -*)
      die "unknown option: $1"
      ;;
    *)
      [ -z "$VERSION" ] || die "unexpected argument: $1"
      VERSION="$1"
      shift
      ;;
  esac
done

[ -n "$VERSION" ] || die "usage: $0 <version> [--run-id <github-run-id>]"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist"
TAP_DIR="$HOME/homebrew-tap"
FORMULA="$TAP_DIR/pueue-for-agents.rb"
BUCKET="tap-ozoxoz"
WORKFLOW="release.yml"
ARTIFACT_PREFIX="pueue-for-agents"
PLATFORMS=("linux amd64" "linux arm64" "darwin amd64" "darwin arm64" "windows amd64" "windows arm64")

for tool in gh t3 jq sha256sum git; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool missing: $tool"
done
[ -d "$TAP_DIR/.git" ] || die "tap repo not found at $TAP_DIR"

cd "$REPO_ROOT"
git rev-parse --git-dir >/dev/null 2>&1 || die "$REPO_ROOT is not a git repo"
git remote get-url origin >/dev/null 2>&1 || die "no git origin in $REPO_ROOT"

# gh resolves this clone to the *upstream* repo by default (it treats a fork's
# parent as the default), so every API call is pinned explicitly.
ORIGIN_URL="$(git remote get-url origin)"
REPO_NWO="$(printf '%s' "$ORIGIN_URL" | sed -E 's#^git@[^:]+:##; s#^https?://[^/]+/##; s#\.git$##')"
printf '%s' "$REPO_NWO" | grep -qE '^[^/]+/[^/]+$' \
  || die "cannot derive OWNER/REPO from origin '$ORIGIN_URL'"

# The artifacts are labelled with the version baked into the binaries at compile
# time (CARGO_PKG_VERSION), so a release argument that disagrees would publish
# mislabelled files.
CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' pueue/Cargo.toml | head -n1)"
[ "$VERSION" = "$CARGO_VERSION" ] \
  || die "version mismatch: release asked for '$VERSION', pueue/Cargo.toml says '$CARGO_VERSION'"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  note "dirty working tree:"
  git status --short --untracked-files=no >&2
  die "commit or stash before releasing — the tag would not match the artifacts"
fi

HEAD_SHA="$(git rev-parse HEAD)"
TAG="v$VERSION"
TAG_EXISTS=0
TAG_PUSHED=0

if git rev-parse "$TAG" >/dev/null 2>&1; then
  [ "$(git rev-parse "$TAG")" = "$HEAD_SHA" ] \
    || die "tag $TAG exists but does not point at HEAD"
  TAG_EXISTS=1
  if git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
    TAG_PUSHED=1
    note "Tag $TAG already exists at HEAD and is pushed — reusing it"
  else
    note "Tag $TAG exists at HEAD locally — pushing it"
  fi
fi

note "=== Releasing $ARTIFACT_PREFIX $VERSION (tag $TAG) ==="

# --- Tag and push -------------------------------------------------------------
if [ -z "$RUN_ID" ]; then
  note "--- Pushing main and tag $TAG ---"
  git push origin HEAD:main
  if [ "$TAG_PUSHED" -eq 0 ]; then
    [ "$TAG_EXISTS" -eq 1 ] || git tag "$TAG"
    git push origin "$TAG"
  fi

  # --- Locate the CI run for this commit -------------------------------------
  note "--- Waiting for the '$WORKFLOW' run for $HEAD_SHA ---"
  for _ in $(seq 1 36); do
    RUN_ID="$(gh run list --repo "$REPO_NWO" --workflow "$WORKFLOW" --commit "$HEAD_SHA" \
      --event push --limit 1 --json databaseId --jq '.[0].databaseId // empty' 2>/dev/null || true)"
    [ -n "$RUN_ID" ] && break
    sleep 5
  done
  [ -n "$RUN_ID" ] || die "no '$WORKFLOW' run registered for $HEAD_SHA within 3 minutes"
else
  note "--- Reusing CI run $RUN_ID (skipping tag/push) ---"
fi

note "  run: $(gh run view "$RUN_ID" --repo "$REPO_NWO" --json url --jq .url)"

# --- Wait for the build -------------------------------------------------------
DEADLINE=$(( $(date +%s) + 3600 ))
while :; do
  STATUS="$(gh run view "$RUN_ID" --repo "$REPO_NWO" --json status --jq .status)"
  [ "$STATUS" = "completed" ] && break
  [ "$(date +%s)" -lt "$DEADLINE" ] || die "CI run $RUN_ID still '$STATUS' after 60 minutes"
  sleep 20
done
CONCLUSION="$(gh run view "$RUN_ID" --repo "$REPO_NWO" --json conclusion --jq .conclusion)"
[ "$CONCLUSION" = "success" ] \
  || die "CI run $RUN_ID finished as '$CONCLUSION' — $(gh run view "$RUN_ID" --repo "$REPO_NWO" --json url --jq .url)"

# --- Download the artifacts ---------------------------------------------------
note "--- Downloading artifacts ---"
mkdir -p "$DIST_DIR"
for platform in "${PLATFORMS[@]}"; do
  set -- $platform
  name="${ARTIFACT_PREFIX}_${VERSION}_$1_$2"
  note "  $name"
  gh run download "$RUN_ID" --repo "$REPO_NWO" --name "$name" --dir "$DIST_DIR" \
    || die "artifact '$name' missing from run $RUN_ID"
done

# --- Checksums ----------------------------------------------------------------
note "--- Generating checksums ---"
CHECKSUMS="$DIST_DIR/checksums_${VERSION}.txt"
for f in "$DIST_DIR"/${ARTIFACT_PREFIX}_${VERSION}_*.tar.gz; do
  [ -f "$f" ] || die "no tarballs downloaded"
  # The CI-computed sidecar is compared against the local hash: a mismatch means
  # a corrupted download, not a checksum to publish.
  sidecar="$f.sha256"
  [ -f "$sidecar" ] || die "missing checksum sidecar for $(basename "$f")"
  want="$(awk '{print $1}' "$sidecar")"
  got="$(sha256sum "$f" | awk '{print $1}')"
  [ "$want" = "$got" ] || die "checksum mismatch for $(basename "$f") (ci $want, local $got)"
  ( cd "$DIST_DIR" && sha256sum "$(basename "$f")" )
done > "$CHECKSUMS"
cat "$CHECKSUMS"

DARWIN_ARM=$(grep 'darwin_arm64' "$CHECKSUMS" | awk '{print $1}')
DARWIN_AMD=$(grep 'darwin_amd64' "$CHECKSUMS" | awk '{print $1}')
LINUX_AMD=$(grep 'linux_amd64' "$CHECKSUMS" | awk '{print $1}')
LINUX_ARM=$(grep 'linux_arm64' "$CHECKSUMS" | awk '{print $1}')
[ -n "$DARWIN_ARM" ] && [ -n "$DARWIN_AMD" ] && [ -n "$LINUX_AMD" ] && [ -n "$LINUX_ARM" ] \
  || die "could not read all four platform checksums"

# --- Upload to Tigris ---------------------------------------------------------
note "--- Uploading artifacts to Tigris bucket '$BUCKET' ---"
for f in "$DIST_DIR"/${ARTIFACT_PREFIX}_${VERSION}_*; do
  key="$(basename "$f")"
  note "  $key"
  t3 cp "$f" "t3://$BUCKET/$key" -a public -y
done

# --- Clean old artifacts from the bucket --------------------------------------
# The bucket is shared with mailbox, viaibot and vidbmcp. Only keys owned by
# this producer are ever removed, matched on the exact `pueue-for-agents_`
# prefix, and only for other versions. Everything else is left untouched —
# a previous unscoped cleanup wiped another product's artifacts.
note "--- Cleaning old $ARTIFACT_PREFIX artifacts (keeping $VERSION) ---"
REMOVED=0
while IFS= read -r key; do
  [ -n "$key" ] || continue
  case "$key" in
    ${ARTIFACT_PREFIX}_*) ;;
    *) continue ;;
  esac
  case "$key" in
    *"$VERSION"*) continue ;;
  esac
  note "  removing $key"
  t3 rm "t3://$BUCKET/$key" -y
  REMOVED=$((REMOVED + 1))
done < <(t3 ls "$BUCKET" --format json 2>/dev/null | jq -r '.items[] | .key // empty')
note "  removed $REMOVED object(s) from $BUCKET; all other keys untouched"

# --- Formula ------------------------------------------------------------------
note "--- Updating Homebrew formula ---"
cat > "$FORMULA" <<'RUBY'
class PueueForAgents < Formula
  desc "Agent-oriented pueue fork — durable task queues for coding agents"
  homepage "https://github.com/Loo0D/pueue-for-agents"
  version "__VERSION__"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://__BUCKET__.t3.tigrisfiles.io/pueue-for-agents___VERSION___darwin_arm64.tar.gz"
      sha256 "__DARWIN_ARM__"
    end
    on_intel do
      url "https://__BUCKET__.t3.tigrisfiles.io/pueue-for-agents___VERSION___darwin_amd64.tar.gz"
      sha256 "__DARWIN_AMD__"
    end
  end

  on_linux do
    on_arm do
      url "https://__BUCKET__.t3.tigrisfiles.io/pueue-for-agents___VERSION___linux_arm64.tar.gz"
      sha256 "__LINUX_ARM__"
    end
    on_intel do
      url "https://__BUCKET__.t3.tigrisfiles.io/pueue-for-agents___VERSION___linux_amd64.tar.gz"
      sha256 "__LINUX_AMD__"
    end
  end

  def install
    bin.install "pueue", "pueued"
  end

  def caveats
    <<~EOS
      This formula installs `pueue` and `pueued`, the same binary names as the
      homebrew-core `pueue` formula, so the two cannot be linked side by side.
      Remove the upstream package first:

        brew uninstall pueue

      The daemon is not started for you. Run `pueued -d`, or supervise it with
      your init system of choice.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/pueue --version")
    assert_match version.to_s, shell_output("#{bin}/pueued --version")
  end
end
RUBY

sed -i \
  -e "s/__VERSION__/${VERSION}/g" \
  -e "s/__BUCKET__/${BUCKET}/g" \
  -e "s/__DARWIN_ARM__/${DARWIN_ARM}/" \
  -e "s/__DARWIN_AMD__/${DARWIN_AMD}/" \
  -e "s/__LINUX_AMD__/${LINUX_AMD}/" \
  -e "s/__LINUX_ARM__/${LINUX_ARM}/" \
  "$FORMULA"
cat "$FORMULA"

# --- Push the formula ---------------------------------------------------------
note "--- Pushing formula to homebrew-tap ---"
if git -C "$TAP_DIR" diff --quiet -- "pueue-for-agents.rb" && \
   git -C "$TAP_DIR" diff --cached --quiet -- "pueue-for-agents.rb" && \
   ! git -C "$TAP_DIR" status --porcelain --untracked-files=all | grep -q 'pueue-for-agents.rb'; then
  note "  formula already up to date — nothing to commit"
else
  git -C "$TAP_DIR" add "pueue-for-agents.rb"
  # Commit only this path so unrelated staged edits in the tap stay untouched.
  git -C "$TAP_DIR" commit -m "Update pueue-for-agents formula to v${VERSION}" -- "pueue-for-agents.rb"
  git -C "$TAP_DIR" push origin HEAD
fi

# --- Clean up -----------------------------------------------------------------
note "--- Cleaning local dist/ (published artifacts stay in Tigris) ---"
rm -rf "$DIST_DIR"

note "=== Release $VERSION complete ==="
note "install: brew install Loo0D/tap/pueue-for-agents"
