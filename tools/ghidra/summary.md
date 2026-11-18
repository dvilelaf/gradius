# Gradius (E) — Ghidra + GhidraNes static analysis

## Setup
- **Ghidra 12.1.4 PUBLIC** (2026-09-21), Java 21 (OpenJDK 21.0.12), headless in `/tmp/opencode/ghidra/ghidra_12.1.4_PUBLIC`.
  Ghidra 12.2 is the latest public release, but **GhidraNes has no 12.2 build** (v20260531 ships
  `ghidra_10.3 … 12.1`), so the exact-compatible pair 12.1.4 + GhidraNes 12.1 was used.
- **GhidraNes v20260531** (`ghidra_12.1_4c41964_20260531_GhidraNes.zip`), unzipped into
  `Ghidra/Extensions/GhidraNes`. Loader name: *NES ROM*; language: `6502:LE:16:default`.
- ROM: `$GRADIUS_ROM` (65552 B, iNES 1.0).
- Headless project: `/tmp/opencode/ghidra-out/proj` (project `gradius`, file `/Gradius (E).nes`).

## Import / analysis result
Works. GhidraNes parses the header, selects `NromMapper` for **mapper 3 (CNROM)**:
PRG 32 KiB mapped once at `$8000-$FFFF` (block `PRG0`, no banking), CHR 32 KiB detected but not
mapped (4×8 KiB banks are only options, no overlay is created). Vectors are applied and become
function entries + labels:

| vector | address |
|---|---|
| RESET  | `$8010` (`reset`) |
| NMI    | `$806A` (`vblank`) |
| IRQ    | `$80B5` (`irq`, 1-byte RTI) |

The loader also creates RAM/PPU/APU blocks and PPU/APU/joypad MMIO labels (`PPUCTRL $2000` … `JOY1 $4016`,
`OAMDMA $4014`), so I/O accesses are symbolic from the start. Total symbols: 414.

Automatic analysis: **47 functions, 2 489 instructions (0x9B9 bytes)**, ~1 s. Everything else in
PRG (≈30 KiB) is left as data. Layout recovered by the export script:

```
CODE   8010-80CB  80CC-817F=DATA  8180-824B  ...  CODE 8B15-8BC7, 8BDE-8BFC, 8C0B-8C7C
DATA   8C7D-EC17  (0x5F9B bytes)   <- level/tile/sprite/text data *plus* undiscovered code
CODE   EC18-ECAB, ECB0-EFB1        <- second code block, reached by JMP $EC18 at $83B2
DATA   EFB2-FFFA                   <- data + vectors
```

**Caveat:** the 0x8C7D-0xEC17 block is not all data. E.g. at `$A2CB` there is a real routine
(`LDA $19 / ASL A / TAY / LDA $A7CA,Y / STA $98 / LDA $A7CB,Y / STA $99 / … / RTS @ $A2F7`) that
Ghidra never disassembled because no direct JSR/JMP reaches it — it is only reachable through
pointer tables. So the 47 functions are the *directly reachable* call graph, not the full code base.

## Key addresses requested
| addr | kind | evidence |
|---|---|---|
| `$8D92` | data — **high-byte half of a 16-bit pointer table at `$8D91`** | `LDX $8D91,Y` @ `$8AB5` and `LDA $8D92,Y` @ `$8AB8` (inside `FUN_PRG0__8ab1`, sprite/OAM builder); pointer pairs resolve to `$8F07,$8F18,$8EF6,$8F29,…` (sprite/entity streams). Indexed by even Y. |
| `$9193` | data (sprite/entity stream `06 00 D1 21 00 00 D1 61 …`) | pointer `93 91` at `$8E81` (odd-indexed pair of the table at `$8E80`) |
| `$A5FC` | data | pointer `FC A5` stored at `$A5FA` |
| `$A65C` | data (start of a pointer/stream table) | pointer `5C A6` stored at `$A5F8`; `$A5F8/$A5FA` = 2-word table `{$A65C,$A5FC}` |
| `$A7CA` | data — **low-byte half of the pair `$A7CA/$A7CB`** | `LDA $A7CA,Y` @ `$A2CF`, `LDA $A7CB,Y` @ `$A2D4`, then `STA $98/$99` → 16-bit pointer used via `LDA ($98),Y`. Entries `$A7D8,$A7E8,$A7F8,$A806,…` |
| `$8B15` | **code** — function `FUN_PRG0__8b15` (0xBA B, 92 insns) | `JSR $8B15` @ `$809F` inside `vblank`; also appears as data word `15 8B` at `$80A0` (the JSR operand) |

Pointer tables that are actually referenced by an instruction operand:
`$8653` (39 entries, `LDA $8653,X` @ `$85FC`) and `$8D91` (dispatch table, `LDX $8D91,Y`).
Repeated idiom in this ROM: a 16-bit pointer table is read as two separate byte loads
(low at base+Y, high at base+1+Y), so tables look "odd aligned" and Ghidra's indexed xref only
shows the low half.

## 6502 decompiler: what it gives / what it does not
`DecompInterface` works for 6502 (`tried=10 ok=10`, see `decompiled.txt`). Output is pseudo-C of
low practical value: every zero-page/absolute pointer becomes
`*(byte *)(ushort)(byte)(param_1 + 4)`, no arrays/structs/RAM variables, no register semantics
(A/X/Y collapse into temporaries), so branch conditions and loops are hard to read. It is usable to
sketch control flow of a single routine, not to reconstruct logic.

What **is** useful: iNES parsing + IO label symbols, function map with sizes/callers, the
code/data split above, and a real xref database — but only after the export script adds the
absolute-operand references, **because the 6502 language creates no operand references at all**
(~350 refs added per run). Without that step `getReferencesTo` returns 0 for `$8D92` etc.

What is missing: jump-table / indirect-dispatch following, data typing of tables, gap-free code
recovery, RAM variable creation, and any call-graph edge that goes through `JMP (zp)`.

## Queries from an agent (no GUI)
```bash
cd /tmp/opencode/ghidra
./ghidra_12.1.4_PUBLIC/support/analyzeHeadless /tmp/opencode/ghidra-out/proj gradius \
  -import "$GRADIUS_ROM" -overwrite \
  -scriptPath /tmp/opencode/ghidra-out/scripts -postScript QueryAddr.java 8D92 8B15 A7CA
```
(prints instruction/function/data/xrefs/pointer-word occurrences for each address)

Full re-export:
```bash
./ghidra_12.1.4_PUBLIC/support/analyzeHeadless /tmp/opencode/ghidra-out/proj gradius \
  -import "$GRADIUS_ROM" -overwrite \
  -scriptPath /tmp/opencode/ghidra-out/scripts -postScript DumpNes.java /tmp/opencode/ghidra-out
```
GhidraMCP/ReVa were not installed (both need a separately built extension/JDK and a live Ghidra
instance; out of the timebox). `QueryAddr.java` covers the "ask by address" use case headlessly.

## Exports
- `functions.txt` — 47 functions: entry, size, instruction count, caller count, name
- `symbols.txt` — 414 symbols (addr, type, namespace, name)
- `refs.txt` — per key address: kind, xrefs (after operand-ref synthesis), operand uses, pointer words
- `codemap.txt` — memory blocks, classification counts, coalesced CODE/DATA/UNDEF ranges, pointer tables
- `disasm.txt` — full instruction listing (2 489 instructions)
- `decompiled.txt` — 10 decompiled functions (pseudo-C probe)
- `scripts/DumpNes.java`, `scripts/QueryAddr.java` — the exporters
- `import.log`, `dump.log` — headless logs (loader/mapper/vector decisions)
