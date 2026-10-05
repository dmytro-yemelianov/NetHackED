#!/usr/bin/env bash
# Build the static site into web/: wasm package, bundled rule packs, schemas.
# Usage: scripts/build-web.sh [--release]
set -euo pipefail
cd "$(dirname "$0")/.."

profile=(--dev)
[[ "${1:-}" == "--release" ]] && profile=(--release)

wasm-pack build crates/nethacked-wasm --target web "${profile[@]}" --out-dir ../../web/pkg
rm -f web/pkg/.gitignore

out=web/packs
rm -rf "$out"
mkdir -p "$out/schema"
cargo run -q -p nethacked-pack --locked -- schema -o "$out/schema"

entries=()
for dir in packs/examples/*/; do
  dir=${dir%/}
  [[ -f "$dir/pack.toml" ]] || continue
  name=$(basename "$dir")
  hash=$(cargo run -q -p nethacked-pack --locked -- build "$dir" -o "$out/$name.nhpack" | tail -1)
  mkdir -p "$out/$name"
  (cd "$dir" && find . -name '*.toml' | sed 's|^\./||') > "$out/$name/files.txt"
  while read -r f; do mkdir -p "$out/$name/$(dirname "$f")"; cp "$dir/$f" "$out/$name/$f"; done < "$out/$name/files.txt"
  id=$(sed -n 's/^id *= *"\(.*\)"/\1/p' "$dir/pack.toml" | head -1)
  version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$dir/pack.toml" | head -1)
  title=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$dir/pack.toml" | head -1)
  entries+=("{\"id\":\"$id\",\"version\":\"$version\",\"title\":\"$title\",\"hash\":\"$hash\",\"file\":\"$name.nhpack\",\"sources\":\"$name/\"}")
done
( IFS=,; echo "[${entries[*]}]" ) > "$out/index.json"
python3 -m json.tool "$out/index.json" >/dev/null
echo "web/ built: $(ls "$out"/*.nhpack | wc -l | tr -d ' ') bundled pack(s)"
