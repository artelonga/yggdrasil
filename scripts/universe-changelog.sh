#!/usr/bin/env bash
# scripts/universe-changelog.sh — Per-universe changelog via git pathspec.
#
# Generates/updates universes/universe-<slug>/CHANGELOG.md from filtered git history.
# jj-compatible: read-only, no history rewriting. See docs/UNIVERSE-VERSIONING.md.
#
# Usage:
#   bash scripts/universe-changelog.sh <slug>
#   bash scripts/universe-changelog.sh <slug> --bump patch|minor|major
#   bash scripts/universe-changelog.sh --all
#   bash scripts/universe-changelog.sh --check
#
# Options:
#   --bump <level>   Freeze [Unreleased] as next version and bump Cargo.toml
#   --all            Run for all universes in REGISTRY.yaml (status: embedded, versions_tracked: true)
#   --check          Exit 1 if any universe changelog is behind commits on its path
#   --use-jj         Prefer 'jj log' for history (falls back to git if jj not found)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
UNIVERSES_DIR="$REPO_ROOT/universes"
REGISTRY="$UNIVERSES_DIR/REGISTRY.yaml"

# ── helpers ──────────────────────────────────────────────────────────────────

die() { echo "ERROR: $*" >&2; exit 1; }

# Reads field value from a universe entry block in REGISTRY.yaml.
# Only searches within the universe's stanza (up to next entry with '- slug:').
# Usage: registry_field <slug> <field>
registry_field() {
    local slug="$1" field="$2"
    awk -v slug="$slug" -v field="$field" '
        /^  - slug:/ { in_block = ($NF == slug) }
        in_block && /^    '"$field"':/ { print $NF; exit }
    ' "$REGISTRY"
}

# Lists all slugs from REGISTRY.yaml that satisfy a filter.
# Usage: list_slugs_where <field> <value>
list_slugs_where() {
    local field="$1" value="$2"
    python3 - "$REGISTRY" "$field" "$value" <<'PYEOF'
import sys, re

registry_path = sys.argv[1]
filter_field  = sys.argv[2]
filter_value  = sys.argv[3]

with open(registry_path) as f:
    content = f.read()

# Split on universe entries
entries = re.split(r'(?=^  - slug:)', content, flags=re.MULTILINE)
for entry in entries:
    slug_m = re.search(r'^  - slug:\s*(\S+)', entry, re.MULTILINE)
    if not slug_m:
        continue
    slug = slug_m.group(1)
    # Check if the field has the desired value
    field_m = re.search(rf'^    {re.escape(filter_field)}:\s*(\S+)', entry, re.MULTILINE)
    if field_m and field_m.group(1) == filter_value:
        print(slug)
PYEOF
}

# Lists slugs where BOTH status==embedded AND versions_tracked==true
list_tracked_slugs() {
    python3 - "$REGISTRY" <<'PYEOF'
import sys, re

with open(sys.argv[1]) as f:
    content = f.read()

entries = re.split(r'(?=^  - slug:)', content, flags=re.MULTILINE)
for entry in entries:
    slug_m = re.search(r'^  - slug:\s*(\S+)', entry, re.MULTILINE)
    if not slug_m:
        continue
    slug = slug_m.group(1)
    status_m = re.search(r'^    status:\s*(\S+)', entry, re.MULTILINE)
    tracked_m = re.search(r'^    versions_tracked:\s*(\S+)', entry, re.MULTILINE)
    if (status_m and status_m.group(1) == 'embedded' and
            tracked_m and tracked_m.group(1) == 'true'):
        print(slug)
PYEOF
}

# Read version from a universe's Cargo.toml
read_version() {
    local cargo_toml="$1"
    grep '^version' "$cargo_toml" | head -1 | sed 's/version = "//;s/"//'
}

# Bump a semver string: bump_version <version> <level>
bump_version() {
    local version="$1" level="$2"
    IFS='.' read -r major minor patch <<< "$version"
    case "$level" in
        major) echo "$((major+1)).0.0" ;;
        minor) echo "$major.$((minor+1)).0" ;;
        patch) echo "$major.$minor.$((patch+1))" ;;
        *) die "Unknown bump level: $level (use major|minor|patch)" ;;
    esac
}

# Determine the most recent git tag for a universe slug.
# Returns empty string if no tags found.
latest_universe_tag() {
    local slug="$1"
    git tag --list "universe-${slug}-v*" 2>/dev/null | sort -V | tail -1
}

# Get commits that touched a universe path since a tag (or all if no tag).
# Outputs lines: "<hash> <subject>"
commits_for_universe() {
    local slug="$1" last_tag="$2"
    local path="universes/universe-${slug}/"
    local range
    if [[ -n "$last_tag" ]]; then
        range="${last_tag}..HEAD"
    else
        range="HEAD"
    fi
    git log "$range" --pretty="%H %s" -- "$path" 2>/dev/null || true
}

# Check if jj is available
jj_available() {
    command -v jj &>/dev/null
}

# ── changelog generation ─────────────────────────────────────────────────────

generate_changelog() {
    local slug="$1"
    local bump_level="${2:-}"
    local universe_dir="$UNIVERSES_DIR/universe-${slug}"
    local changelog="$universe_dir/CHANGELOG.md"
    local cargo_toml="$universe_dir/Cargo.toml"

    [[ -d "$universe_dir" ]] || die "Universe not found: universe-${slug}"
    [[ -f "$cargo_toml"   ]] || die "Cargo.toml not found: $cargo_toml"

    local current_version
    current_version="$(read_version "$cargo_toml")"
    [[ -n "$current_version" ]] || die "Could not read version from $cargo_toml"

    local last_tag
    last_tag="$(latest_universe_tag "$slug")"

    local range_desc
    if [[ -n "$last_tag" ]]; then
        range_desc="${last_tag}..HEAD"
    else
        range_desc="all commits"
    fi

    echo "==> Generating universes/universe-${slug}/CHANGELOG.md from git history"
    echo "    Range: ${range_desc}"

    # Collect commits
    local raw_commits
    raw_commits="$(commits_for_universe "$slug" "$last_tag")"

    # Classify commits by Conventional Commit type + scope filter
    local added=() changed=() fixed=() uncategorised=()

    while IFS= read -r line; do
        [[ -z "$line" ]] && continue
        local hash subject
        hash="${line%% *}"
        subject="${line#* }"

        # Match: feat(universe-slug), feat(slug), feat(YG-N, slug), feat(YG-N):, or path fallback
        local type scope cc_pat
        cc_pat='^([a-z]+)\(([^)]+)\):'
        if [[ "$subject" =~ $cc_pat ]]; then
            type="${BASH_REMATCH[1]}"
            scope="${BASH_REMATCH[2]}"
            # Accept scopes: "universe-<slug>", "<slug>", or containing "<slug>"
            if [[ "$scope" == "universe-${slug}" || "$scope" == "$slug" || "$scope" == *"$slug"* ]]; then
                local msg="${subject#*): }"
                case "$type" in
                    feat)     added+=("$msg") ;;
                    fix)      fixed+=("$msg") ;;
                    refactor|chore|docs|test) changed+=("$msg") ;;
                    *)        uncategorised+=("$msg") ;;
                esac
                continue
            fi
        fi

        # Fallback: any commit that touched the path (already filtered by git log)
        uncategorised+=("$subject")
    done <<< "$raw_commits"

    # Build [Unreleased] block inline (avoid bash 4.3 nameref requirement)
    local jj_note=""
    jj_available && jj_note=" <!-- regenerated via git pathspec; jj log universes/universe-${slug}/ also works -->"

    local unreleased_block
    unreleased_block="## [Unreleased]${jj_note}"$'\n'

    local has_entries=false
    [[ ${#added[@]} -gt 0 || ${#changed[@]} -gt 0 || ${#fixed[@]} -gt 0 || ${#uncategorised[@]} -gt 0 ]] && has_entries=true

    if [[ "$has_entries" == false ]]; then
        unreleased_block+=$'\n''_Nenhuma mudança pendente._'$'\n'
    else
        if [[ ${#added[@]} -gt 0 ]]; then
            unreleased_block+=$'\n''### Added'$'\n'
            for item in "${added[@]}"; do unreleased_block+="- ${item}"$'\n'; done
        fi
        if [[ ${#changed[@]} -gt 0 ]]; then
            unreleased_block+=$'\n''### Changed'$'\n'
            for item in "${changed[@]}"; do unreleased_block+="- ${item}"$'\n'; done
        fi
        if [[ ${#fixed[@]} -gt 0 ]]; then
            unreleased_block+=$'\n''### Fixed'$'\n'
            for item in "${fixed[@]}"; do unreleased_block+="- ${item}"$'\n'; done
        fi
        if [[ ${#uncategorised[@]} -gt 0 ]]; then
            unreleased_block+=$'\n''### Other'$'\n'
            for item in "${uncategorised[@]}"; do unreleased_block+="- ${item}"$'\n'; done
        fi
    fi

    if [[ -n "$bump_level" ]]; then
        # Freeze as a numbered release
        local new_version
        new_version="$(bump_version "$current_version" "$bump_level")"

        # Validate new version > current
        if ! version_gt "$new_version" "$current_version"; then
            die "New version $new_version is not greater than current $current_version"
        fi

        local today
        today="$(date +%Y-%m-%d)"
        unreleased_block="${unreleased_block//\[Unreleased\]/[${new_version}] — ${today}}"

        update_changelog_file "$changelog" "$unreleased_block" "$slug"

        # Bump Cargo.toml
        sed -i.bak "s/^version = \"${current_version}\"/version = \"${new_version}\"/" "$cargo_toml"
        rm -f "${cargo_toml}.bak"

        echo "    Bumped: ${current_version} → ${new_version}"
        echo "    ✅ Updated $changelog"
        echo "    ✅ Bumped $cargo_toml"
    else
        update_changelog_file "$changelog" "$unreleased_block" "$slug"
        echo "    ✅ Updated $changelog"
    fi
}

# Upsert [Unreleased] section in a CHANGELOG.md.
# If the file has no [Unreleased] section, prepend it after the header lines.
update_changelog_file() {
    local changelog="$1" unreleased_block="$2" slug="$3"

    if [[ ! -f "$changelog" ]]; then
        # Create new file
        {
            echo "# Changelog — universe-${slug}"
            echo
            echo "Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)."
            echo "SemVer. Versionamento independente do core (ver \`docs/UNIVERSE-VERSIONING.md\`)."
            echo
            echo "$unreleased_block"
        } > "$changelog"
        return
    fi

    # Replace existing [Unreleased] block, or insert after the header
    python3 - "$changelog" "$unreleased_block" <<'PYEOF'
import sys, re

changelog_path = sys.argv[1]
new_block = sys.argv[2]

with open(changelog_path) as f:
    content = f.read()

# Match existing [Unreleased] section (up to next ## [ heading or EOF)
unreleased_pat = re.compile(
    r'## \[Unreleased\][^\n]*\n.*?(?=\n## \[|\Z)',
    re.DOTALL
)

if unreleased_pat.search(content):
    # rstrip + '\n' ensures exactly one trailing newline before the lookahead \n## [
    new_content = unreleased_pat.sub(new_block.rstrip('\n') + '\n', content, count=1)
else:
    # Insert right before the first ## [ versioned section
    first_version_pat = re.compile(r'^## \[', re.MULTILINE)
    m = first_version_pat.search(content)
    if m:
        insert_at = m.start()
        new_content = content[:insert_at] + new_block + '\n' + content[insert_at:]
    else:
        # Append at end
        new_content = content.rstrip('\n') + '\n\n' + new_block + '\n'

with open(changelog_path, 'w') as f:
    f.write(new_content)
PYEOF
}

# Compare semver: returns 0 (true) if v1 > v2
version_gt() {
    local v1="$1" v2="$2"
    python3 -c "
import sys
v1 = tuple(int(x) for x in '$v1'.split('.'))
v2 = tuple(int(x) for x in '$v2'.split('.'))
sys.exit(0 if v1 > v2 else 1)
"
}

# ── check mode ───────────────────────────────────────────────────────────────

check_all() {
    local failed=0
    local slugs=()
    while IFS= read -r s; do [[ -n "$s" ]] && slugs+=("$s"); done < <(list_tracked_slugs)

    if [[ ${#slugs[@]} -eq 0 ]]; then
        echo "No universes with versions_tracked: true found in REGISTRY.yaml"
        return 0
    fi

    for slug in "${slugs[@]}"; do
        local universe_dir="$UNIVERSES_DIR/universe-${slug}"
        [[ -d "$universe_dir" ]] || continue

        local last_tag
        last_tag="$(latest_universe_tag "$slug")"
        local raw_commits
        raw_commits="$(commits_for_universe "$slug" "$last_tag")"

        if [[ -z "$raw_commits" ]]; then
            continue
        fi

        # Check if CHANGELOG has an [Unreleased] section
        local changelog="$universe_dir/CHANGELOG.md"
        if [[ ! -f "$changelog" ]]; then
            echo "  FAIL universe-${slug}: CHANGELOG.md missing (${raw_commits//
/; })"
            failed=1
            continue
        fi

        if ! grep -q '## \[Unreleased\]' "$changelog" 2>/dev/null; then
            echo "  FAIL universe-${slug}: commits since $last_tag not reflected in CHANGELOG.md"
            failed=1
        else
            echo "  OK   universe-${slug}"
        fi
    done

    if [[ $failed -ne 0 ]]; then
        echo ""
        echo "Some universe changelogs are out of date. Run: bash scripts/universe-changelog.sh --all"
        return 1
    fi

    echo "All universe changelogs up to date."
}

# ── main ─────────────────────────────────────────────────────────────────────

main() {
    cd "$REPO_ROOT"

    local mode="" slug="" bump_level=""

    if [[ $# -eq 0 ]]; then
        echo "Usage:"
        echo "  $0 <slug>                        Generate/update CHANGELOG for one universe"
        echo "  $0 <slug> --bump patch|minor|major   Freeze [Unreleased] as new version"
        echo "  $0 --all                         Generate/update for all tracked universes"
        echo "  $0 --check                       Exit 1 if any changelog is behind commits"
        exit 0
    fi

    case "$1" in
        --all)   mode="all" ;;
        --check) mode="check" ;;
        *)       mode="single"; slug="$1" ;;
    esac

    shift

    # Parse remaining options
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --bump)
                [[ -n "${2:-}" ]] || die "--bump requires a level: patch|minor|major"
                bump_level="$2"
                shift 2
                ;;
            --use-jj)
                # Noted but git is always available; jj auto-detected via PATH
                shift
                ;;
            *)
                die "Unknown option: $1"
                ;;
        esac
    done

    case "$mode" in
        single)
            [[ -n "$slug" ]] || die "No slug provided"
            generate_changelog "$slug" "$bump_level"
            ;;
        all)
            local slugs=()
            while IFS= read -r s; do [[ -n "$s" ]] && slugs+=("$s"); done < <(list_tracked_slugs)
            if [[ ${#slugs[@]} -eq 0 ]]; then
                echo "No universes with status: embedded and versions_tracked: true found."
                exit 0
            fi
            echo "==> Running for ${#slugs[@]} tracked universes: ${slugs[*]}"
            for s in "${slugs[@]}"; do
                # Only process universes that have a directory
                local udir="$UNIVERSES_DIR/universe-${s}"
                if [[ -d "$udir" ]]; then
                    generate_changelog "$s" "$bump_level"
                else
                    echo "  SKIP universe-${s}: directory not found"
                fi
            done
            ;;
        check)
            check_all
            ;;
    esac
}

main "$@"
