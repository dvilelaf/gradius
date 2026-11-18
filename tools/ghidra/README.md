# Ghidra + GhidraNes analysis

Static cross-check of the original cartridge, used to find function boundaries, pointer tables
and code/data gaps that a trace-only view misses. Complements `xtask disasm`; it does not replace
the oracle.

## Versions used

- Ghidra 12.1.4 PUBLIC (Java 21).
- GhidraNes `v20260531` (`ghidra_12.1` build), loader name **NES ROM**. Mapper 3 (CNROM) is
  recognized as `NromMapper` with PRG fixed at `$8000-$FFFF`.
- Ghidra 12.2 exists but has no matching GhidraNes release yet.

## Reproduce

```bash
GHIDRA_HOME=/path/to/ghidra_12.1.4_PUBLIC
ROM="/path/to/Gradius (E).nes"
PROJ=/tmp/ghidra-gradius

# Full export: functions, symbols, refs, code map, disassembly, decompiler probe.
"$GHIDRA_HOME/support/analyzeHeadless" "$PROJ" gradius \
  -import "$ROM" -overwrite \
  -scriptPath tools/ghidra -postScript DumpNes.java /tmp/ghidra-out

# Pointer-table recovery: adds functions/labels, re-runs analysis and writes
# functions_full.txt + pointer_tables.txt (leaves functions.txt untouched).
"$GHIDRA_HOME/support/analyzeHeadless" "$PROJ" gradius \
  -import "$ROM" -overwrite \
  -scriptPath tools/ghidra -postScript RecoverCode.java /tmp/ghidra-out

# Ask about specific addresses without a GUI.
"$GHIDRA_HOME/support/analyzeHeadless" "$PROJ" gradius \
  -import "$ROM" -overwrite \
  -scriptPath tools/ghidra -postScript QueryAddr.java 8D92 8B15 A7CA
```

## What the exports say

- The raw import has 47 directly reachable functions (1,228 instructions,
  2,489 code bytes). `RecoverCode.java` follows the inline `JSR $83E9` tables
  and raises this to 214 functions / 6,636 instructions; see
  `docs/analysis/ghidra.md` for the method and the oracle check.
- `$8C7D-$EC17` is *not* all data: it contains pointer-table-reached code (for example the
  spawner at `$A2CB`) that no direct `JSR`/`JMP` reaches. The raw import's 47 is a lower bound;
  the recovered 214-function map is the current best static view.
- `$8D91` is the low byte of a 16-bit pointer table (`$8D92` is the high byte), indexed by an
  even Y inside the OAM builder at `$8AB1`.
- `$A7CA/$A7CB` is the low/high pair of the stage section table used by the spawner.
- The 6502 decompiler runs but produces low-quality pseudo-C; the value is in the function map,
  code/data split and synthesized operand xrefs.

Files kept in this directory: `summary.md`, `functions.txt`, `functions_full.txt`,
`symbols.txt`, `pointer_tables.txt`, `refs.txt` and the Java exporters. Full listings stay
outside the repo. No cartridge bytes are stored here.

`symbols.txt` is the stable `address name size` form written by `cargo xtask symbols`;
`cargo xtask disasm --symbols tools/ghidra/symbols.txt` uses it (and
`disasm`/`trace` fall back to `functions_full.txt` when `--symbols` is omitted).
