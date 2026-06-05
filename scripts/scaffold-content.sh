#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)/content"

scaffold_category() {
  local category="$1"
  local count="$2"
  local label="$3"
  local desc="$4"
  for lang in "" shandara tagmar godot; do
    local dir="$ROOT/$category${lang:+/$lang}"
    mkdir -p "$dir"
    if [ -z "$lang" ]; then
      local universe_name="$label"
      local universe_desc="$desc"
    else
      local universe_name="$label ($lang)"
      local universe_desc="$desc — $lang variant"
    fi
    if [ ! -f "$dir/_universe.yaml" ]; then
      cat > "$dir/_universe.yaml" <<YAML
name: "$universe_name"
description: "$universe_desc"
version: 0
content_types: []
properties: {}
YAML
    fi
  done
  for i in $(seq 1 "$count"); do
    local f="$ROOT/$category/$i.md"
    if [ ! -f "$f" ]; then
      printf '%s\n' "---" "id: $i" "name: \"Placeholder $label $i\"" "status: stub" "---" > "$f"
    fi
  done
}

scaffold_category npc       15 "NPC"      "Non-player characters across all worlds"
scaffold_category monster   15 "Monster"  "Monsters and creatures across all worlds"
scaffold_category location  10 "Location" "Locations and maps across all worlds"
scaffold_category item      10 "Item"     "Items and equipment across all worlds"
scaffold_category faction    5 "Faction"  "Factions and organizations across all worlds"
scaffold_category encounter  5 "Encounter" "Encounter tables and combat scenarios"

echo "Scaffold complete: $ROOT"
