#!/usr/bin/env python3
"""Write THIRD-PARTY-LICENSES.txt for one target of the `gradius` binary.

Lists every registry crate that the binary links (normal dependencies only,
for the given target and features), SDL2 and the embedded Inter font, each with its license. A license
text that several crates share is printed once. The texts come from the
LICENSE*/COPYING* files that the crates ship in the cargo registry.

    scripts/third-party-licenses.py --target x86_64-pc-windows-gnu \
        --out target/third-party-windows.txt
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import textwrap

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PROJECT = "gradius-sdl"  # the package that builds the `gradius` binary
INTER_LICENSE = os.path.join(ROOT, "crates", PROJECT, "fonts", "OFL.txt")  # Inter, embedded with include_bytes!
LICENSE_FILE = re.compile(r"(LICEN[CS]E|COPYING|UNLICEN[CS]E|NOTICE)", re.IGNORECASE)
# sdl2-sys ships no LICENSE file: its MIT part is rust-sdl2's, which the sdl2 crate of the same version ships.
SIBLING_LICENSE = {"sdl2-sys": "sdl2"}


def metadata(target, features):
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target]
    if features:
        cmd += ["--features", features]
    out = subprocess.run(cmd, cwd=ROOT, check=True, stdout=subprocess.PIPE, text=True).stdout
    return json.loads(out)


def linked_ids(meta):
    """Ids of the packages the binary links: normal dependencies, transitively."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == PROJECT and p["source"] is None)
    seen, todo = set(), [root]
    while todo:
        for dep in nodes[todo.pop()]["deps"]:
            if dep["pkg"] not in seen and any(k["kind"] is None for k in dep["dep_kinds"]):
                seen.add(dep["pkg"])
                todo.append(dep["pkg"])
    return seen


def license_files(pkg):
    folder = os.path.dirname(pkg["manifest_path"])
    if pkg["name"] in SIBLING_LICENSE:
        folder = os.path.join(os.path.dirname(folder), f"{SIBLING_LICENSE[pkg['name']]}-{pkg['version']}")
    names = sorted(n for n in os.listdir(folder)
                   if LICENSE_FILE.match(n) and os.path.isfile(os.path.join(folder, n)))
    if not names:
        raise SystemExit(f"third-party-licenses: {pkg['name']} {pkg['version']} ships no license file "
                         f"({pkg['license']}); add its text by hand before packaging")
    return [os.path.join(folder, n) for n in names]


def sdl_from_sources(meta, linked):
    """LICENSE.txt and version of the SDL2 source that sdl2-sys bundles."""
    pkg = next(p for p in meta["packages"] if p["id"] in linked and p["name"] == "sdl2-sys")
    sdl = os.path.join(os.path.dirname(pkg["manifest_path"]), "SDL")
    header = open(os.path.join(sdl, "include", "SDL_version.h"), encoding="utf-8").read()
    parts = [re.search(rf"#define SDL_{part}\s+(\d+)", header).group(1)
             for part in ("MAJOR_VERSION", "MINOR_VERSION", "PATCHLEVEL")]
    return os.path.join(sdl, "LICENSE.txt"), ".".join(parts)


def read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().strip("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--target", required=True)
    parser.add_argument("--features", default="", help="cargo features of the build")
    parser.add_argument("--out", required=True)
    parser.add_argument("--sdl-license", help="SDL2 license file (default: the bundled SDL source)")
    parser.add_argument("--sdl-version", help="SDL2 version (default: the bundled SDL source)")
    args = parser.parse_args()

    meta = metadata(args.target, args.features)
    linked = linked_ids(meta)
    project = next(p for p in meta["packages"] if p["name"] == PROJECT and p["source"] is None)
    crates = sorted((p for p in meta["packages"] if p["id"] in linked and p["source"]),
                    key=lambda p: (p["name"], p["version"]))
    bundled_license, bundled_version = sdl_from_sources(meta, linked)
    sdl_license = args.sdl_license or bundled_license
    sdl_version = args.sdl_version or bundled_version

    # A text is stored once, keyed by its content; `users` names the components that carry it.
    texts, components = {}, []

    def text_number(label, body, user):
        digest = hashlib.sha256(body.encode("utf-8")).hexdigest()
        if digest not in texts:
            texts[digest] = {"number": len(texts) + 1, "label": label, "body": body, "users": []}
        texts[digest]["users"].append(user)
        return texts[digest]["number"]

    for pkg in crates:
        user = f"{pkg['name']} {pkg['version']}"
        numbers = [text_number(os.path.basename(f), read(f), user) for f in license_files(pkg)]
        components.append((user, pkg["license"] or "UNKNOWN", numbers))
    sdl_number = text_number(os.path.basename(sdl_license), read(sdl_license), f"SDL2 {sdl_version}")
    components.append((f"SDL2 {sdl_version}", "Zlib", [sdl_number]))
    inter_number = text_number(os.path.basename(INTER_LICENSE), read(INTER_LICENSE), "Inter (font)")
    components.append(("Inter (font)", "OFL-1.1", [inter_number]))

    lines = [
        f"Third-party software in gradius {project['version']} ({args.target})",
        "",
        "Generated by scripts/third-party-licenses.py. Gradius itself is MIT OR Apache-2.0: see",
        "LICENSE-MIT and LICENSE-APACHE. Each component below is followed by the numbers of its",
        "license texts, printed once each in the second part.",
        "",
        f"Components ({len(components)})",
        "-" * 40,
    ]
    width = max(len(name) for name, _, _ in components)
    for name, license_, numbers in components:
        refs = ", ".join(f"[{n}]" for n in numbers)
        lines.append(f"{name.ljust(width)}  {license_}  {refs}")

    lines += ["", "License texts", "-" * 40]
    for entry in sorted(texts.values(), key=lambda t: t["number"]):
        lines += ["", f"[{entry['number']}] {entry['label']}",
                  textwrap.fill("Used by: " + ", ".join(entry["users"]), 78), "", entry["body"]]

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"third-party-licenses: {len(components)} components, {len(texts)} texts -> {args.out}")


if __name__ == "__main__":
    main()
