#!/usr/bin/env bash
# Decide whether a PR is a pure release version bump (lean CI).
# Fail-safe: any error or doubt yields lean=false, and the script always exits 0.
# Inputs (env): EVENT_NAME HEAD_REPO REPO HEAD_REF BASE_SHA HEAD_SHA GITHUB_OUTPUT
set -u

norm_toml() { # replace the [package] version value with a placeholder
  awk 'BEGIN{s="";d=0} /^\[/{s=$0} s=="[package]" && !d && /^version = "/{print "version = \"X\"";d=1;next} {print}'
}
norm_json() {
  awk '!d && /^  "version": "/{print "  \"version\": \"X\",";d=1;next} {print}'
}
norm_lock() {
  awk 'p{print "version = \"X\"";p=0;next} /^name = "pup"$/{p=1} {print}'
}
get_toml() { awk 'BEGIN{s=""} /^\[/{s=$0} s=="[package]" && /^version = "/{gsub(/^version = "|"$/,"");print;exit}'; }
get_json() { awk '/^  "version": "/{gsub(/^  "version": "|",?$/,"");print;exit}'; }
get_lock() { awk 'p{gsub(/^version = "|"$/,"");print;exit} /^name = "pup"$/{p=1}'; }

detect() {
  [[ "$EVENT_NAME" == "pull_request" ]] || return 1
  [[ -n "$HEAD_REPO" && "$HEAD_REPO" == "$REPO" ]] || return 1
  [[ "$HEAD_REF" =~ ^release/v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || return 1
  local ver="${BASH_REMATCH[1]}"
  [[ -n "$BASE_SHA" && -n "$HEAD_SHA" ]] || return 1
  git cat-file -e "$BASE_SHA^{commit}" 2>/dev/null || git fetch -q --no-tags origin "$BASE_SHA" 2>/dev/null || return 1
  git cat-file -e "$HEAD_SHA^{commit}" 2>/dev/null || git fetch -q --no-tags origin "$HEAD_SHA" 2>/dev/null || return 1
  local mb; mb="$(git merge-base "$BASE_SHA" "$HEAD_SHA")" || return 1
  [[ -n "$mb" ]] || return 1
  local changes; changes="$(git diff --name-status --no-renames "$mb" "$HEAD_SHA")" || return 1
  [[ "$(printf '%s\n' "$changes" | sort)" == "$(printf 'M\tCargo.lock\nM\tCargo.toml\nM\tplugin.json')" ]] || return 1
  local f
  for f in Cargo.toml plugin.json Cargo.lock; do
    local n=norm_toml g=get_toml
    [[ $f == plugin.json ]] && { n=norm_json; g=get_json; }
    [[ $f == Cargo.lock ]] && { n=norm_lock; g=get_lock; }
    local old new
    old="$(git show "$mb:$f" | $n)" || return 1
    new="$(git show "$HEAD_SHA:$f" | $n)" || return 1
    [[ -n "$old" && "$old" == "$new" ]] || return 1
    [[ "$(git show "$HEAD_SHA:$f" | $g)" == "$ver" ]] || return 1
  done
  return 0
}

lean=false
if ( detect ) 2>/dev/null; then lean=true; fi
echo "lean=$lean" >> "${GITHUB_OUTPUT:-/dev/stdout}"
exit 0
