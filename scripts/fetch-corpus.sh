#!/usr/bin/env bash
# Fetch the tier-2 corpus: real Project Gutenberg epubs, into the gitignored
# corpus/epub/.
#
# Gutenberg is volunteer-run and blocks clients that hammer it, so this is
# strictly sequential, rate-limited, and identifies itself. Files already
# present are skipped, so re-running is cheap and polite.
#
#   scripts/fetch-corpus.sh            fetch what's missing, verify checksums
#   scripts/fetch-corpus.sh --record   fetch, then write the sha256s back into
#                                      the manifest (do this once, then commit)
#
# Checksums matter: pinning them means a Gutenberg re-issue fails loudly here
# instead of silently reshuffling every generated golden downstream.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/corpus/manifest.json"
DEST="$ROOT/corpus/epub"
UA="readingbuddy-corpus/0.1 (test fixture generator; contact via repository)"
RATE="${CORPUS_RATE:-500k}"

RECORD=0
[[ "${1:-}" == "--record" ]] && RECORD=1

command -v jq >/dev/null || { echo "error: jq is required" >&2; exit 1; }
mkdir -p "$DEST"

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

count=$(jq '.books | length' "$MANIFEST")
echo "corpus: $count books -> $DEST"

fetched=0; skipped=0; failed=0
declare -a RECORDED=()

for i in $(seq 0 $((count - 1))); do
  id=$(jq -r ".books[$i].id" "$MANIFEST")
  title=$(jq -r ".books[$i].title" "$MANIFEST")
  want=$(jq -r ".books[$i].sha256 // empty" "$MANIFEST")
  out="$DEST/$id.epub"

  if [[ -f "$out" ]]; then
    skipped=$((skipped + 1))
  else
    # The /cache/epub/ path is the direct file. The /ebooks/ form redirects and
    # is the documented fallback for books the cache has not materialised.
    # (Note `pg$id.epub3.images` 404s — that naming is gone.)
    printf '  %-6s %s\n' "$id" "$title"
    ok=0
    for url in \
      "https://www.gutenberg.org/cache/epub/$id/pg$id.epub" \
      "https://www.gutenberg.org/ebooks/$id.epub3.images" \
      "https://www.gutenberg.org/ebooks/$id.epub.noimages"
    do
      if curl -fsSL --limit-rate "$RATE" -A "$UA" --max-time 120 -o "$out.part" "$url"; then
        ok=1
        break
      fi
    done
    if (( ! ok )); then
      echo "    FAILED $id (no epub at any known URL)" >&2
      rm -f "$out.part"
      failed=$((failed + 1))
      continue
    fi
    mv "$out.part" "$out"
    fetched=$((fetched + 1))
    sleep 1   # be a good citizen
  fi

  got=$(sha "$out")
  if [[ -n "$want" && "$want" != "$got" ]]; then
    echo "    CHECKSUM MISMATCH for $id" >&2
    echo "      expected $want" >&2
    echo "      got      $got" >&2
    echo "    Gutenberg re-issued this text. Review, then re-record." >&2
    failed=$((failed + 1))
  fi
  RECORDED+=("$id=$got")
done

if (( RECORD )); then
  tmp=$(mktemp)
  cp "$MANIFEST" "$tmp"
  for pair in "${RECORDED[@]}"; do
    id="${pair%%=*}"; digest="${pair#*=}"
    jq --argjson id "$id" --arg d "$digest" \
       '.books |= map(if .id == $id then .sha256 = $d else . end)' \
       "$tmp" > "$tmp.next" && mv "$tmp.next" "$tmp"
  done
  mv "$tmp" "$MANIFEST"
  echo "recorded ${#RECORDED[@]} checksums into corpus/manifest.json — commit it"
fi

echo "fetched=$fetched skipped=$skipped failed=$failed"
(( failed == 0 )) || exit 1
