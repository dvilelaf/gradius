#!/usr/bin/env bash
# Maps where the port diverges from the cartridge over a long run.
#
#   scripts/longrun.sh [--rom PATH] [--pack PATH] [--out DIR] [--frames N]
#
# Two lockstep runs of frames 0..N-1 (default 12000):
#
#   default  the harness default script, no keep-alive. The oracle dies at
#            2709/3232/3755, game-overs and runs the attract cycle; the port
#            lives past the second death and then parks in GameOver.
#   boss     scripts/boss-script.txt with --keep-alive --native-keep-alive, so
#            both ships survive into the Big Core; the oracle reaches stage 2.
#
# Each run writes NAME.txt (the full report), NAME.json (summary), NAME.err and
# the per-frame state dumps NAME.oracle / NAME.native. Pixels are sampled every
# 30th frame with --pixel-report. buckets.txt summarises both runs per 1000
# frames and lists the first divergence of every field.
#
# The cartridge is read in place and never copied. Set GRADIUS_ROM or pass
# --rom; GRADIUS_PACK and CARGO_TARGET_DIR are honoured like the other scripts.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
rom=${GRADIUS_ROM:-}
pack=${GRADIUS_PACK:-"$root/assets/gradius.pak"}
out=${OUT:-/tmp/opencode/longrun}
frames=12000

while (($#)); do
    case $1 in
        --rom) rom=${2:?--rom needs a value}; shift 2 ;;
        --pack) pack=${2:?--pack needs a value}; shift 2 ;;
        --out) out=${2:?--out needs a value}; shift 2 ;;
        --frames) frames=${2:?--frames needs a value}; shift 2 ;;
        -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
        *) echo "longrun: unknown argument $1" >&2; exit 2 ;;
    esac
done

[[ -n $rom ]] || { echo "longrun: set GRADIUS_ROM or pass --rom" >&2; exit 2; }
[[ -f $rom ]] || { echo "longrun: no ROM at $rom" >&2; exit 2; }
[[ -f $pack ]] || { echo "longrun: no pack at $pack (cargo xtask extract)" >&2; exit 2; }
command -v python3 >/dev/null || { echo "longrun: python3 not found" >&2; exit 2; }

diff_bin="${CARGO_TARGET_DIR:-$root/target}/release/gradius-diff"
if [[ ! -x $diff_bin ]]; then
    (cd "$root" && cargo build --release -p gradius-diff)
fi

mkdir -p "$out"

run() { # NAME [extra args...]
    local name=$1
    shift
    local status=0
    "$diff_bin" --rom "$rom" --pack "$pack" --frames "$frames" \
        --json "$out/$name.json" \
        --oracle-out "$out/$name.oracle" --native-out "$out/$name.native" \
        --pixel-report --png-every 30 "$@" \
        >"$out/$name.txt" 2>"$out/$name.err" || status=$?
    # 1 means the run diverged, which is the point; 2 is a usage/I/O failure.
    if ((status > 1)); then
        echo "longrun: $name failed with status $status" >&2
        cat "$out/$name.err" >&2
        exit 1
    fi
    echo "longrun: $name done ($frames frames, exit $status)"
}

run default
run boss --script "$root/scripts/boss-script.txt" \
    --keep-alive --native-keep-alive

python3 - "$out/default" "$out/boss" >"$out/buckets.txt" <<'PY'
import collections
import statistics
import sys


def parse(path):
    out = []
    for line in open(path):
        row = {"objects": []}
        for field in line.split():
            key, value = field.split("=", 1)
            if key == "objects":
                if value:
                    row["objects"] = [tuple(map(int, o.split(":"))) for o in value.split(";")]
            elif key == "scroll":
                row["scroll"] = tuple(map(int, value.split(":")))
            elif key == "state":
                row["state"] = value
            else:
                row[key] = int(value)
        out.append(row)
    return out


def diverging(oracle, native):
    """Fields compared by gradius-diff, in its order; capsule is excluded."""
    bad = []
    if abs((oracle["px"] * 256 + oracle["pf"]) - (native["px"] * 256 + native["pf"])) > 256:
        bad.append("px")
    if abs((oracle["py"] * 256 + oracle["pyf"]) - (native["py"] * 256 + native["pyf"])) > 256:
        bad.append("py")
    for field in ("speed", "score", "lives", "meter"):
        if oracle[field] != native[field]:
            bad.append(field)
    if oracle["scroll"] != native["scroll"]:
        bad.append("scroll")
    oracle_slots = {o[0]: o for o in oracle["objects"]}
    native_slots = {o[0]: o for o in native["objects"]}
    for slot in set(oracle_slots) | set(native_slots):
        if slot in oracle_slots and slot in native_slots:
            a, b = oracle_slots[slot], native_slots[slot]
            if a[1] != b[1]:
                bad.append("obj.kind")
            if abs(a[2] - b[2]) > 1:
                bad.append("obj.x")
            if abs(a[3] - b[3]) > 1:
                bad.append("obj.y")
            if a[4] != b[4]:
                bad.append("obj.substate")
        else:
            bad.append("obj.slot")
    return bad


def runs(frames):
    out = []
    start = prev = None
    for frame in frames:
        if start is None:
            start = frame
        elif frame != prev + 1:
            out.append((start, prev))
            start = frame
        prev = frame
    if start is not None:
        out.append((start, prev))
    return " ".join(f"{a}-{b}" if a != b else str(a) for a, b in out) or "-"


def summarise(name, base):
    oracle = parse(base + ".oracle")
    native = parse(base + ".native")
    pixels = collections.defaultdict(list)
    for line in open(base + ".txt"):
        parts = line.split()
        if parts and parts[0].isdigit():
            pixels[int(parts[0]) // 1000].append(
                [float(value.rstrip("%")) for value in parts[1:5]]
            )

    print(f"\n## {name}")
    print("| bucket | cmp | state | first state | first in window | slot pairs | live set | pixel | play | hud | samples |")
    print("| --- | ---: | ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |")
    for bucket in range((len(oracle) + 999) // 1000):
        lo, hi = bucket * 1000, bucket * 1000 + 1000
        oo, nn = oracle[lo:hi], native[lo:hi]
        cmp = sum(1 for o, n in zip(oo, nn) if o["state"] == n["state"] == "Playing")
        same_state = sum(1 for o, n in zip(oo, nn) if o["state"] == n["state"])
        first = "-"
        first_state = "-"
        for o, n in zip(oo, nn):
            if o["state"] == n["state"] == "Playing":
                bad = diverging(o, n)
                if bad:
                    first = f"{o['frame']} {bad[0]}" + (" cont." if o["frame"] == lo + 1 else "")
                    break
        for o, n in zip(oo, nn):
            if o["state"] != n["state"]:
                first_state = f"{o['frame']} {o['state']}->{n['state']}"
                break
        pairs = matches = 0
        for o, n in zip(oo, nn):
            a = {ob[0] for ob in o["objects"]}
            b = {ob[0] for ob in n["objects"]}
            pairs += len(a | b)
            matches += len(a & b)
        live_sets = sum(
            1 for o, n in zip(oo, nn)
            if {ob[0] for ob in o["objects"]} == {ob[0] for ob in n["objects"]}
        )
        samples = pixels.get(bucket, [])
        mean = lambda index: statistics.mean(s[index] for s in samples) if samples else 0.0
        rate = f"{100 * matches / pairs:.1f}%" if pairs else "-"
        print(
            f"| {lo + 1}-{hi} | {cmp} | {same_state}/{hi - lo} | {first_state} | {first} | "
            f"{matches}/{pairs} ({rate}) | {live_sets}/{hi - lo} | "
            f"{mean(1):.1f}% | {mean(2):.1f}% | {mean(3):.1f}% | {len(samples)} |"
        )

    firsts = {}
    for o, n in zip(oracle, native):
        if o["state"] == n["state"] == "Playing":
            for field in diverging(o, n):
                firsts.setdefault(field, o["frame"])
    print("\nfirst divergence by field (both Playing):")
    for field, frame in sorted(firsts.items(), key=lambda item: item[1]):
        print(f"- {field}: {frame}")

    state_runs = []
    start = prev = None
    last = None
    for row in oracle:
        if row["state"] != last:
            if last is not None:
                state_runs.append((last, start, row["frame"] - 1))
            last, start = row["state"], row["frame"]
    state_runs.append((last, start, oracle[-1]["frame"]))
    print("\noracle state runs: " + ", ".join(f"{s} {a}-{b}" for s, a, b in state_runs))

    for side, rows in (("oracle", oracle), ("native", native)):
        previous = None
        changes = []
        for row in rows:
            scroll = row["scroll"]
            label = f"{scroll[0]}:{scroll[1]}:{scroll[2]}"
            position = scroll[1] * 256 + scroll[0] + scroll[2] / 256
            if previous is not None and position + 128 < previous[1]:
                changes.append(f"{row['frame']} {previous[0]}->{label}")
            previous = (label, position)
        print(f"{side} scroll resets: {' '.join(changes) or '-'}")

        previous = None
        changes = []
        for row in rows:
            if row["lives"] != previous:
                changes.append(f"{row['frame']} {previous}->{row['lives']}")
                previous = row["lives"]
        print(f"{side} lives changes: {' '.join(changes)}")

        present = collections.defaultdict(list)
        for row in rows:
            for obj in row["objects"]:
                present[obj[1]].append(row["frame"])
        print(
            f"{side} object kinds (object-frames first-last): "
            + " ".join(
                f"{kind}:{len(frames)} {frames[0]}-{frames[-1]}"
                for kind, frames in sorted(present.items())
            )
        )

    mismatched = [o["frame"] for o, n in zip(oracle, native) if o["state"] != n["state"]]
    print(f"state mismatch frames: {len(mismatched)} in {runs(mismatched)}")
    counts = [
        o["frame"]
        for o, n in zip(oracle, native)
        if o["state"] == n["state"] == "Playing"
        and len(o["objects"]) != len(n["objects"])
    ]
    print(f"object count mismatch frames (Playing): {len(counts)} in {runs(counts)}")


for name, base in (("default", sys.argv[1]), ("boss", sys.argv[2])):
    summarise(name, base)
PY

echo "longrun: wrote $out/{default,boss}.{txt,json,oracle,native,err} and $out/buckets.txt"
