#!/usr/bin/env bash
# Owner-only authorship and attribution policy for Nexonic AI.
# Used by the local Git hooks and by CI (.github/workflows/guard.yml).
#
#   owner-policy.sh identity            check the configured author/committer
#   owner-policy.sh message <file>      check one commit message
#   owner-policy.sh range <base> <head> check every commit in base..head
set -euo pipefail

OWNER_NAME="Shahrukh Khalid"
OWNER_EMAIL="90670731+Shahrukhalid@users.noreply.github.com"
# GitHub's own committer for merges/squashes made on github.com. It is
# platform metadata, not source authorship, so it is allowed as committer only.
GITHUB_COMMITTER_EMAIL="noreply@github.com"

# Attribution lines that must never appear in commit messages.
BANNED='co-authored-by:.*(claude|anthropic|copilot|codex|openai|chatgpt|gemini|cursor|antigravity|devin|\[bot\])|generated (with|by) .*(claude|ai|copilot|codex)|claude\.(ai|com)/code|claude-session:|🤖'

fail() { echo "owner-policy: $*" >&2; exit 1; }

check_message() {
  if grep -qiE "$BANNED" "$1"; then
    grep -niE "$BANNED" "$1" >&2 || true
    fail "commit message contains tool attribution (see lines above)."
  fi
}

case "${1:-}" in
  identity)
    name="$(git config user.name || true)"; email="$(git config user.email || true)"
    [ "$name" = "$OWNER_NAME" ] || fail "user.name is '$name', expected '$OWNER_NAME'."
    [ "$email" = "$OWNER_EMAIL" ] || fail "user.email is '$email', expected '$OWNER_EMAIL'."
    ;;
  message)
    check_message "$2"
    ;;
  range)
    base="$2"; head="$3"; bad=0
    if [ -z "$base" ] || [ "$base" = "0000000000000000000000000000000000000000" ]; then
      revs="$(git rev-list "$head")"
    elif ! git cat-file -e "$base^{commit}" 2>/dev/null; then
      # The branch was rewritten (force-push): the old tip no longer exists.
      # Check every commit on the branch that is not already on main.
      main_ref="$(git rev-parse -q --verify origin/main || git rev-parse -q --verify main || true)"
      if [ -n "$main_ref" ]; then
        echo "owner-policy: previous tip $base is gone (rewritten branch); checking commits not on main." >&2
        revs="$(git rev-list "$main_ref..$head")"
      else
        revs="$(git rev-list "$head")"
      fi
    else
      revs="$(git rev-list "$base..$head")"
    fi
    tmp="$(mktemp)"
    for c in $revs; do
      ae="$(git log -1 --format=%ae "$c")"; an="$(git log -1 --format=%an "$c")"
      ce="$(git log -1 --format=%ce "$c")"
      if [ "$ae" != "$OWNER_EMAIL" ] || [ "$an" != "$OWNER_NAME" ]; then
        echo "  $c author '$an <$ae>' is not the owner" >&2; bad=1
      fi
      if [ "$ce" != "$OWNER_EMAIL" ] && [ "$ce" != "$GITHUB_COMMITTER_EMAIL" ]; then
        echo "  $c committer '$ce' is not allowed" >&2; bad=1
      fi
      git log -1 --format=%B "$c" > "$tmp"
      if grep -qiE "$BANNED" "$tmp"; then
        echo "  $c message contains tool attribution" >&2; bad=1
      fi
    done
    rm -f "$tmp"
    [ "$bad" = 0 ] || fail "one or more commits break the owner-only rule."
    echo "owner-policy: $(echo "$revs" | grep -c . || true) commit(s) checked, all owner-authored."
    ;;
  *)
    fail "usage: owner-policy.sh identity | message <file> | range <base> <head>"
    ;;
esac
