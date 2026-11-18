#!/usr/bin/env bash
# Build the public repository as a new git history with one commit.
#
#   scripts/prepare-public-repo.sh [--author "Name <email>"] [--message "text"] <destination>
#
# The tree is HEAD as `git archive` exports it, so paths marked export-ignore in
# .gitattributes are left out. The history, tags and branches of this clone are
# not copied. Author defaults to git config (GIT_AUTHOR_* variables also work).
# Nothing is pushed.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
author=""
message="Initial public release"
dest=""

usage() {
    echo "usage: $0 [--author \"Name <email>\"] [--message \"text\"] <destination>" >&2
    exit 2
}

die() {
    echo "prepare-public-repo: $*" >&2
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --author) author="${2:?--author needs a value}"; shift 2 ;;
        --message) message="${2:?--message needs a value}"; shift 2 ;;
        -h | --help) usage ;;
        -*) usage ;;
        *) [[ -z "$dest" ]] || usage; dest="$1"; shift ;;
    esac
done
[[ -n "$dest" ]] || usage
dest="$(realpath -m -- "$dest")"

cd "$root"
[[ -z "$(git status --porcelain)" ]] || die "the tree is not clean; commit or remove the changes listed by git status"
case "$dest/" in
    "$root/"*) die "$dest is inside this repository" ;;
esac
if [[ -e "$dest" ]] && { [[ ! -d "$dest" ]] || [[ -n "$(ls -A "$dest")" ]]; }; then
    die "$dest exists and is not an empty directory"
fi
mkdir -p "$dest"

git archive HEAD | tar -x -C "$dest"
cd "$dest"
git init -q -b main
git add -A
commit_args=(-q -m "$message")
[[ -z "$author" ]] || commit_args+=(--author "$author")
git commit "${commit_args[@]}"

fail=0
problem() {
    echo "prepare-public-repo: $*" >&2
    fail=1
}

(scripts/check-rom-data.sh) || problem "scripts/check-rom-data.sh fails"
[[ "$(git log --all --oneline | wc -l)" == 1 ]] || problem "history is not a single commit"
[[ "$(git for-each-ref --format='%(refname)')" == refs/heads/main ]] || problem "refs other than refs/heads/main exist"
[[ -z "$(git remote)" ]] || problem "remotes are configured"
[[ -e HANDOFF.md || -e docs/plans ]] && problem "HANDOFF.md or docs/plans is present"
while IFS= read -r -d '' file; do
    file="${file#./}"
    case "$file" in
        *.nes | *.pak | *.grs | tests/fixtures/*.state | tests/fixtures/*.trace)
            problem "game data in the tree: $file" ;;
    esac
done < <(find . -path ./.git -prune -o -type f -print0)

((fail == 0)) || die "checks failed; $dest is not ready to publish"

echo "public repo ready in $dest: $(git ls-files | wc -l) files, $(du -sh --exclude=.git . | cut -f1), one commit on main, no tags or remotes"
echo "Nothing was pushed. Once the empty remote exists, publish with:"
echo "  cd $dest && git remote add origin <url> && git push -u origin main"
