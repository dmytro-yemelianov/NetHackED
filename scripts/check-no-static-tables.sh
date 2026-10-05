#!/usr/bin/env bash
# Production code outside netrust-data must read game data through a Ruleset.
set -euo pipefail
pattern='\b(BESTIARY|ITEM_CATALOG|ROLES|RACES|C_ARMOR|get_role_quest_config|monster_archetype_by_name|item_archetype_by_name|get_monster_species|get_item_archetype|spawn_player_character)\b'
hits=$(grep -rnE "$pattern" crates/*/src --include='*.rs' \
  | grep -v '^crates/netrust-data/' \
  | grep -v '^crates/netrust-core/' \
  | grep -v ':[0-9]*:\s*//' \
  | grep -v '\.spawn_player_character' \
  | grep -v '// static-ok' || true)
if [ -n "$hits" ]; then
  echo "Static game-data access outside a Ruleset:"
  echo "$hits"
  exit 1
fi
echo "No static game-data access outside netrust-data."
