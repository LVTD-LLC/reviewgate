#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "validate-skills: $*" >&2
  exit 1
}

if [[ ! -d skills ]]; then
  fail "skills/ directory is missing"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

found=0

while IFS= read -r -d '' skill; do
  found=1

  first_line="$(sed -n '1p' "$skill")"
  [[ "$first_line" == "---" ]] || fail "$skill: missing opening YAML frontmatter fence"

  frontmatter_end="$(awk 'NR > 1 && $0 == "---" { print NR; exit }' "$skill")"
  [[ -n "$frontmatter_end" ]] || fail "$skill: missing closing YAML frontmatter fence"

  if (( frontmatter_end <= 2 )); then
    fail "$skill: empty YAML frontmatter"
  fi

  frontmatter="$(sed -n "2,$((frontmatter_end - 1))p" "$skill")"
  grep -Eq '^name: [A-Za-z0-9_-]+$' <<<"$frontmatter" || fail "$skill: missing simple name field"
  grep -Eq '^description: .+' <<<"$frontmatter" || fail "$skill: missing description field"

  fence_count="$(grep -c '^```' "$skill" || true)"
  if (( fence_count % 2 != 0 )); then
    fail "$skill: unbalanced fenced code block"
  fi

  slug="${skill//[^A-Za-z0-9]/_}"
  awk -v outdir="$tmp_dir" -v slug="$slug" '
    /^```(bash|sh|shell)$/ {
      inside = 1
      n += 1
      file = sprintf("%s/%s-%03d.sh", outdir, slug, n)
      next
    }
    /^```$/ && inside {
      inside = 0
      file = ""
      next
    }
    inside {
      print > file
    }
  ' "$skill"
done < <(find skills -mindepth 2 -maxdepth 2 -name SKILL.md -print0)

if (( found == 0 )); then
  fail "no skills/*/SKILL.md files found"
fi

for block in "$tmp_dir"/*.sh; do
  [[ -e "$block" ]] || continue
  if grep -Eq '(^|[^A-Za-z0-9_])gh([[:space:]]|$)' "$block"; then
    bash -n "$block" || fail "$block: bash syntax check failed"
  fi
done

echo "validate-skills: ok"
