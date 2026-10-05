#!/usr/bin/env bash
# Build the clean keyboard-only game for Cloudflare into deploy/cloudflare/dist/.
# Usage: scripts/build-cf.sh   (then: cd deploy/cloudflare && npx wrangler deploy)
set -euo pipefail
cd "$(dirname "$0")/.."

scripts/build-web.sh --release
dist=deploy/cloudflare/dist
rm -rf "$dist"
mkdir -p "$dist"
# The page lives at the site root on Cloudflare, so pkg/ and packs/ are siblings.
sed 's|<meta name="netrust-base" content="../">|<meta name="netrust-base" content="./">|' web/play/index.html > "$dist/index.html"
grep -q 'content="./"' "$dist/index.html"
cp web/play/play.js "$dist/"
cp -R web/pkg "$dist/pkg"
rm -f "$dist/pkg/.gitignore" "$dist/pkg/package.json" "$dist/pkg/"*.d.ts
cp -R web/packs "$dist/packs"
cat > "$dist/_headers" <<'HDR'
/*
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'
/pkg/*
  Cache-Control: public, max-age=3600
HDR
echo "dist: $(du -sh "$dist" | cut -f1) in $dist"
