#!/usr/bin/env bash
# Fail if any relative markdown link in a tracked *.md file points at a missing path.
#
# Known limitations (this is a simple grep-based checker):
#   * links inside code fences are not skipped and may be reported
#   * targets containing spaces or written as <...> are not handled
#   * reference-style links ([text][ref]) are not checked
#   * #fragments are stripped, not validated
#   * link targets containing nested parentheses are truncated
set -u
cd "$(git rev-parse --show-toplevel)" || exit 2
broken=0
while IFS= read -r file; do
  dir=$(dirname "$file")
  # extract link targets: ](target)
  while IFS= read -r target; do
    [ -z "$target" ] && continue
    target=${target%% *}          # drop optional link title
    case "$target" in
      http://*|https://*|mailto:*|\#*) continue ;;
    esac
    path=${target%%#*}            # strip #fragment
    [ -z "$path" ] && continue
    if [ ! -e "$dir/$path" ]; then
      echo "BROKEN: $file -> $target"
      broken=1
    fi
  done < <(grep -oE '\]\([^)]+\)' "$file" | sed -E 's/^\]\(//; s/\)$//')
done < <(git ls-files '*.md')
if [ "$broken" -ne 0 ]; then
  echo "Broken documentation links found." >&2
  exit 1
fi
echo "All relative markdown links resolve."
