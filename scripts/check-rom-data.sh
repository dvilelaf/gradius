#!/usr/bin/env bash
# Guard against committing data taken from the cartridge.
#
# A tracked Rust file is flagged when, outside comments and test code, it has
#   - a bracketed literal with more than 15 numbers,
#   - a byte string longer than 32 bytes, or
#   - more than 256 numbers inside bracketed literals in total.
# Tracked *.nes, *.pak and tests/fixtures/*.state|*.trace files are always flagged.
#
# Every flagged path must be listed in scripts/rom-data-allow.txt under the card
# that moves its data out of the repository. An entry whose file is no longer
# flagged, or no longer exists, fails too, so the list cannot rot.
set -euo pipefail
cd "$(dirname "$0")/.."

exec python3 - scripts/rom-data-allow.txt <<'PY'
import fnmatch
import re
import subprocess
import sys

MAX_BRACKET = 15
MAX_BYTES = 32
MAX_TOTAL = 256
DATA_FILES = ["*.nes", "*.pak", "tests/fixtures/*.state", "tests/fixtures/*.trace"]

NUM = r"(?<![\w.])(?:0x[0-9a-fA-F_]+|0b[01_]+|0o[0-7_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?)(?:[iuf](?:8|16|32|64|128|size))?(?!\w)"
TOKEN = re.compile(r"\[|\]|" + NUM)
OPEN = re.compile(r'(b?)(r?)(#*)"')
CHAR = re.compile(r"'(?:\\.[^'\n]*|[^\\'\n])'")
CFG_TEST = re.compile(r"^[ \t]*#\[cfg\(test\)\]", re.MULTILINE)


def blank(text):
    return re.sub(r"[^\n]", " ", text)


def byte_len(body, raw):
    if raw:
        return len(body)
    count, i = 0, 0
    while i < len(body):
        if body[i] == "\\" and body[i + 1] == "\n":
            i += 2
            while i < len(body) and body[i] in " \t\r\n":
                i += 1
            count += 1
            continue
        i += 4 if body.startswith("\\x", i) else 2 if body[i] == "\\" else 1
        count += 1
    return count


def scan(text):
    """Blank comments and literal contents, keeping every offset.

    Returns the blanked code and the (offset, length) of each byte string.
    """
    out, byte_strings = [], []
    i, n = 0, len(text)
    while i < n:
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif text.startswith("/*", i):
            depth, j = 0, i
            while j < n:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                    if depth == 0:
                        break
                else:
                    j += 1
            out.append(blank(text[i:j]))
            i = j
        elif text[i] == "'" and (m := CHAR.match(text, i)):
            out.append("'" + " " * (m.end() - i - 2) + "'")
            i = m.end()
        elif text[i] in "br\"#" and (m := OPEN.match(text, i)):
            prefix, raw, hashes = m.group(1), m.group(2), m.group(3)
            start = m.end()
            if raw:
                close = '"' + hashes
                end = text.find(close, start)
                if end < 0:
                    raise ValueError(f"unterminated raw string at offset {i}")
            else:
                close = '"'
                end = start
                while end < n and text[end] != '"':
                    end += 2 if text[end] == "\\" else 1
                if end >= n:
                    raise ValueError(f"unterminated string at offset {i}")
            body = text[start:end]
            if prefix == "b":
                byte_strings.append((i, byte_len(body, raw)))
            out.append(text[i:start] + blank(body) + close)
            i = end + len(close)
        else:
            out.append(text[i])
            i += 1
    return "".join(out), byte_strings


def measure(code, byte_strings):
    """Return (worst bracket count, numbers inside brackets, longest byte string)."""
    stack, worst, total = [], 0, 0
    for m in TOKEN.finditer(code):
        tok = m.group(0)
        if tok == "[":
            stack.append(0)
        elif tok == "]":
            if stack:
                worst = max(worst, stack.pop())
        elif stack:
            total += 1
            stack = [count + 1 for count in stack]
    worst = max([worst] + stack)
    longest = max([length for _, length in byte_strings] or [0])
    return worst, total, longest


def reasons_for(path, text):
    if "/tests/" in "/" + path or path.endswith("tests.rs"):
        return []
    code, byte_strings = scan(text)
    cut = CFG_TEST.search(code)
    if cut:
        byte_strings = [item for item in byte_strings if item[0] < cut.start()]
        code = code[: cut.start()]
    worst, total, longest = measure(code, byte_strings)
    reasons = []
    if worst > MAX_BRACKET:
        reasons.append(f"bracketed literal with {worst} numbers (max {MAX_BRACKET})")
    if longest > MAX_BYTES:
        reasons.append(f"byte string of {longest} bytes (max {MAX_BYTES})")
    if total > MAX_TOTAL:
        reasons.append(f"{total} numbers in bracketed literals (max {MAX_TOTAL})")
    return reasons


tracked = subprocess.run(
    ["git", "ls-files", "-z"], check=True, capture_output=True, text=True
).stdout.split("\0")
findings = {}
for path in filter(None, tracked):
    is_data = any(fnmatch.fnmatch(path, pat) for pat in DATA_FILES)
    if not (path.endswith(".rs") or is_data):
        continue
    if is_data and not path.endswith(".rs"):
        findings[path] = ["ROM-derived data file tracked by git"]
        continue
    try:
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
    except FileNotFoundError:
        continue
    reasons = reasons_for(path, text)
    if reasons:
        findings[path] = reasons

allow_file = sys.argv[1]
try:
    with open(allow_file, encoding="utf-8") as handle:
        lines = handle.read().splitlines()
except FileNotFoundError:
    lines = []

problems, allowed, card = [], {}, None
for number, line in enumerate(lines, 1):
    text = line.strip()
    if not text:
        continue
    if header := re.fullmatch(r"# (\S+)", text):
        card = header.group(1)
        continue
    if text.startswith("#"):
        continue
    parts = text.split(None, 2)
    if len(parts) < 3:
        problems.append(f"{allow_file}:{number}: malformed entry, expected '<path> <card> <reason>'")
    elif card is None or parts[1] != card:
        problems.append(f"{allow_file}:{number}: entry {parts[0]} must sit under its '# {parts[1]}' header")
    elif parts[0] in allowed:
        problems.append(f"{allow_file}:{number}: duplicate entry {parts[0]}")
    else:
        allowed[parts[0]] = number

for path in sorted(findings):
    if path not in allowed:
        reasons = "; ".join(findings[path])
        problems.append(f"{path}: {reasons}; move the data out or list it in {allow_file}")
for path, number in sorted(allowed.items(), key=lambda item: item[1]):
    if path not in findings:
        problems.append(f"{allow_file}:{number}: stale entry {path}: no longer flagged; remove it")

if problems:
    for problem in problems:
        print(f"rom-data: {problem}", file=sys.stderr)
    sys.exit(1)
print(f"rom-data: ok ({len(findings)} allowlisted findings)")
PY
