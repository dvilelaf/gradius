#!/usr/bin/env bash
# Find a Node.js new enough for the web (wasm demo) gates.
#
# Source this file, then call `find_node`: it prints the path of a node binary
# whose major version is at least NODE_MIN_MAJOR (default 18) and returns 0, or
# prints nothing and returns 1. The `node` on PATH wins when it is new enough,
# so a shell already inside nvm/fnm/asdf is respected; otherwise the installed
# versions those managers keep on disk are scanned for the newest one. That
# covers an nvm install (NVM_DIR/versions/node) without sourcing nvm.sh, which
# would only reorder PATH.

node_min_major=${NODE_MIN_MAJOR:-18}

node_major() {
    local version
    version=$("$1" --version 2>/dev/null) || version=
    version=${version#v}
    printf '%s\n' "${version%%.*}"
}

node_new_enough() {
    local major
    major=$(node_major "$1")
    [[ -n $major ]] && ((major >= node_min_major))
}

find_node() {
    local candidate dir

    candidate=$(command -v node 2>/dev/null) || candidate=
    if [[ -n $candidate ]] && node_new_enough "$candidate"; then
        printf '%s\n' "$candidate"
        return 0
    fi

    # nvm: one directory per version under NVM_DIR/versions/node.
    # fnm: <version>/installation/bin/node under its node-versions directory.
    for dir in "${NVM_DIR:-$HOME/.nvm}/versions/node" \
        "$HOME/.local/share/fnm/node-versions"; do
        [[ -d $dir ]] || continue
        candidate=$(find "$dir" -maxdepth 4 -path '*/bin/node' -type f -perm -u+x \
            2>/dev/null | sort -V | tail -1)
        if [[ -n $candidate ]] && node_new_enough "$candidate"; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    # Volta and asdf put shims on PATH, but a bare non-login shell misses them.
    for candidate in "$HOME/.volta/bin/node" "$HOME/.asdf/shims/node"; do
        if [[ -x $candidate ]] && node_new_enough "$candidate"; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    return 1
}
