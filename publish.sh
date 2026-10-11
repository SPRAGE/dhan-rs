#!/usr/bin/env zsh
set -euo pipefail

# =============================================================================
# dhan-rs — Release Script
#
# Validates, tags, and pushes to trigger GitHub Actions publishing.
# The actual crates.io publish happens in CI via the publish.yml workflow.
#
# Usage:
#   ./publish.sh              # Release current version from Cargo.toml
#   ./publish.sh 0.2.0        # Bump to 0.2.0, then release
#   ./publish.sh --dry-run    # Run all checks without actually releasing
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info()  { echo "${CYAN}▸${NC} $1"; }
ok()    { echo "${GREEN}✔${NC} $1"; }
warn()  { echo "${YELLOW}⚠${NC} $1"; }
err()   { echo "${RED}✖${NC} $1" >&2; }
die()   { err "$1"; exit 1; }

DRY_RUN=false
NEW_VERSION=""

# Parse args
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        --help|-h)
            echo "Usage: ./publish.sh [VERSION] [--dry-run]"
            echo ""
            echo "  VERSION    Optional semver (e.g. 0.2.0). If omitted, uses Cargo.toml."
            echo "  --dry-run  Run all checks without publishing or tagging."
            exit 0
            ;;
        *) NEW_VERSION="$arg" ;;
    esac
done

# ---------------------------------------------------------------------------
# 1. Pre-flight checks
# ---------------------------------------------------------------------------

info "Running pre-flight checks..."

# Must be in repo root
[[ -f "Cargo.toml" ]] || die "Run this script from the repository root."

# Must be on main branch
BRANCH=$(git branch --show-current)
[[ "$BRANCH" == "main" ]] || die "Must be on 'main' branch (currently on '$BRANCH')."

# A version argument must not bypass review of tracked or untracked changes.
if [[ -n "$(git status --porcelain)" ]]; then
    die "Working tree has uncommitted changes. Commit or stash them first."
fi

# Cargo and git must be available
command -v cargo &>/dev/null || die "cargo not found."
command -v git &>/dev/null   || die "git not found."

ok "Pre-flight checks passed"

# ---------------------------------------------------------------------------
# 2. Version handling
# ---------------------------------------------------------------------------

CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
info "Current version in Cargo.toml: ${CYAN}${CURRENT_VERSION}${NC}"

if [[ -n "$NEW_VERSION" ]]; then
    # Validate semver format
    if [[ ! "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
        die "Invalid version format: '$NEW_VERSION'. Use semver (e.g. 0.2.0)."
    fi

    if [[ "$NEW_VERSION" == "$CURRENT_VERSION" ]]; then
        warn "Version is already $NEW_VERSION, skipping bump."
    else
        $DRY_RUN && die "Dry runs do not bump versions. Prepare $NEW_VERSION first, or omit VERSION."
        info "Bumping version: ${CURRENT_VERSION} → ${NEW_VERSION}"
        sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${NEW_VERSION}\"/" Cargo.toml

        # Also update doc(html_root_url) in lib.rs if present
        if grep -q "html_root_url" src/lib.rs 2>/dev/null; then
            sed -i "s|docs.rs/dhan-rs/${CURRENT_VERSION}|docs.rs/dhan-rs/${NEW_VERSION}|" src/lib.rs
        fi

        # Update lockfile
        cargo generate-lockfile --quiet

        git add Cargo.toml Cargo.lock src/lib.rs
        git commit -m "chore: bump version to ${NEW_VERSION}"
        ok "Version bumped to ${NEW_VERSION}"
    fi
    VERSION="$NEW_VERSION"
else
    VERSION="$CURRENT_VERSION"
fi

TAG="v${VERSION}"
RELEASE_COMMIT=$(git rev-parse HEAD)
TAG_EXISTS=false

# A failed push may leave a valid local tag. Never replace a different tag.
if git show-ref --verify --quiet "refs/tags/$TAG"; then
    [[ "$(git rev-parse "${TAG}^{commit}")" == "$RELEASE_COMMIT" ]] ||
        die "Git tag '$TAG' points at a different commit. Review the release state."
    TAG_EXISTS=true
    warn "Reusing local tag $TAG at the validated commit."
fi

# ---------------------------------------------------------------------------
# 3. Quality checks
# ---------------------------------------------------------------------------

info "Running cargo check..."
cargo check --locked --all-targets --all-features --quiet
ok "cargo check passed"

info "Running cargo fmt check..."
cargo fmt --all -- --check || die "cargo fmt found issues. Run 'cargo fmt' first."
ok "cargo fmt passed"

info "Running clippy..."
cargo clippy --locked --all-targets --all-features --quiet -- -D warnings || die "clippy found warnings."
ok "clippy passed"

info "Running tests..."
cargo test --locked --all-targets --all-features --quiet
cargo test --locked --doc --all-features --quiet
ok "Tests passed"

info "Building documentation..."
RUSTDOCFLAGS="-Dwarnings" cargo doc --locked --no-deps --all-features --quiet
ok "Documentation builds cleanly"

# ---------------------------------------------------------------------------
# 4. Package verification
# ---------------------------------------------------------------------------

info "Verifying crate package..."
cargo package --locked --quiet
ok "Package verified (${VERSION})"

PACKAGE_SIZE=$(du -h "${CARGO_TARGET_DIR:-target}/package/dhan-rs-${VERSION}.crate" 2>/dev/null | cut -f1)
info "Package size: ${PACKAGE_SIZE:-unknown}"

# ---------------------------------------------------------------------------
# 5. Publish
# ---------------------------------------------------------------------------

if $DRY_RUN; then
    echo ""
    warn "Dry run — skipping tag creation and publish."
    info "Would publish tag: ${TAG} at ${RELEASE_COMMIT}"
    info "Would publish: dhan-rs ${VERSION}"
    echo ""
    info "To publish for real, run:"
    echo "  ./publish.sh ${VERSION}"
    exit 0
fi

[[ -z "$(git status --porcelain)" ]] || die "Checks changed the working tree. Review and commit before releasing."
[[ "$(git rev-parse HEAD)" == "$RELEASE_COMMIT" ]] || die "HEAD changed during checks. Restart the release."

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Ready to release ${CYAN}dhan-rs v${VERSION}${NC}"
echo ""
echo "  This will:"
echo "    1. Create or reuse git tag ${CYAN}${TAG}${NC}"
echo "    2. Atomically push main and the tag to origin"
echo "    3. GitHub Actions will validate and publish to ${CYAN}crates.io${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
printf "Continue? [y/N] "
read -r CONFIRM
[[ "$CONFIRM" =~ ^[Yy]$ ]] || { info "Aborted."; exit 0; }

# Both remote refs succeed together; a retry can reuse the exact local tag.
if ! $TAG_EXISTS; then
    info "Creating tag ${TAG}..."
    git tag -a "$TAG" "$RELEASE_COMMIT" -m "Release ${VERSION}"
    ok "Tag created: ${TAG}"
fi

info "Publishing main and ${TAG} atomically..."
git push --atomic origin "${RELEASE_COMMIT}:refs/heads/main" "refs/tags/${TAG}:refs/tags/${TAG}"
ok "Release refs synchronized — check GitHub Actions for publication"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ${GREEN}✔ dhan-rs v${VERSION} release refs synchronized!${NC}"
echo ""
echo "  🚀 GitHub Actions will run CI and publish automatically."
echo "  🔗 https://github.com/SPRAGE/dhan-rs/actions"
echo ""
echo "  Once published:"
echo "  📦 https://crates.io/crates/dhan-rs"
echo "  📖 https://docs.rs/dhan-rs"
echo "  🏷️  https://github.com/SPRAGE/dhan-rs/releases/tag/${TAG}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
