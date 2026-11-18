# Ghidra pointer-table code recovery

Ghidra + GhidraNes sees the direct `JSR`/`JMP` call graph, but most of
`$8C7D-$EC17` is reached indirectly. This note records the second pass that
follows the indirect tables, the decode guard that keeps sprite/stage bytes from
becoming fake functions, and the oracle check on the result.

The original export (`tools/ghidra/functions.txt`) is unchanged. The recovered
list is `tools/ghidra/functions_full.txt`; the table evidence is
`tools/ghidra/pointer_tables.txt`. No cartridge bytes are stored in the repo.

## Toolchain and baseline

- Ghidra 12.1.4 PUBLIC + GhidraNes `v20260531` (the matching 12.1 build),
  headless from `/tmp/opencode/ghidra`.
- ROM: `Gradius (E).nes`, 65,552 B, mapper 3 (CNROM), SHA-256
  `268dc4b5b344e06e96281eb3159c1f61d3011c4f21dd1a2b4508aa934aff6143`.
- Baseline analysis: 47 functions, 1,228 instructions, 2,489 code bytes
  (`# Classification: instructions=2489` in the old code map counts code bytes,
  not instructions).

## Method

`tools/ghidra/RecoverCode.java` runs as a post-script after the normal import:

1. Adds absolute-operand data references (the 6502 language creates none).
2. Scans PRG for `20 E9 83` (`JSR $83E9`). `$83E9` is an inline dispatcher: it
   pops its return address, reads a 16-bit pointer from the table that follows
   the `JSR`, and ends in `JMP ($98)`. The table is data, but the dispatcher's
   return address points at it, so no ordinary flow analysis can separate them.
3. Also follows the tables named by earlier analysis:
   `$8D91`, `$8E80`/`$8E81`, `$A7CA`/`$A7CB`, `$A5F8`.
4. Decodes every candidate entry with `PseudoDisassembler`, creates a function
   and a `PT_*` label for accepted entries, then runs auto-analysis again. It
   also closes over direct `JSR`/`JMP` targets found inside recovered bodies.
5. Trims inline-table bytes out of any function body that the linear decoder
   swallowed (the table lies between a `JSR $83E9` and the following code).

The nested decoder is the false-positive guard. Starting at a candidate it
follows fallthrough, conditional branches and known jumps; it rejects `BRK`,
`JAM`, undefined/illegal opcodes, addresses outside PRG, runs over 500
instructions, and any path that never reaches `RTS`/`RTI`/`JMP`. A table stops
at the first non-decodable word. This matters because sprite, stage and text
data are full of LE16 values such as `$8F07` or `$9193` that decode for a few
bytes; isolated 16-bit pairs are not accepted merely because they decode. Only
anchored tables (the inline `JSR $83E9` blocks or the named tables) are followed.

## Tables followed

| table | kind | words | decoded entries | recovery |
|---|---|---:|---:|---|
| `$80CC` | inline `JSR $83E9` @ `$80C9` | 7 | 7 | 7 |
| `$88B2` | inline `JSR $83E9` @ `$88AF` | 7 | 4 | 4 |
| `$898E` | inline `JSR $83E9` @ `$898B` | 7 | 7 | 6 functions; `$899C` stays shadowed by `FUN_8979`'s inline table |
| `$96B8` | inline `JSR $83E9` @ `$96B5` | 5 | 5 | 5 |
| `$9822` | inline `JSR $83E9` @ `$981F` | 16 | 16 | 16 |
| `$AE16` | inline `JSR $83E9` @ `$AE13` | 42 | 42 | 42 |
| `$C433` | inline `JSR $83E9` @ `$C430` | 11 | 7 | 7 |
| `$8D91` | sprite-group pointers | 164 | 0 | data only |
| `$8E80`/`$8E81` | sprite/entity streams | 2 + 44 | 0 | data only |
| `$A7CA`/`$A7CB` | stage-section pointers | 68 | 0 | data only |
| `$A5F8` | two-word table `$A65C/$A5FC` | 2 | 0 | data only |

The named tables are real pointer tables, but their entries point at sprite or
stage streams, not at code. They are followed and rejected by the decoder; this
is why they contribute no functions.

## Result

- 7 inline dispatch tables, 75 accepted 16-bit entries, 138 functions created
  directly by the script. Auto-analysis finishes the call graph: **214 functions
  total** (167 new; 47 were already in `functions.txt`).
- **1,228 -> 6,636 instructions**, 2,489 -> 14,267 code bytes.
- `functions_full.txt` records entry, size, instruction count, callers, name,
  `origin` (`direct`, `ptr`, `call`) and the table/call-site evidence.

The recovered code includes the stage-section spawner at `$A2CB`, reached as a
fallthrough of the `$A2BA` routine that is itself called from recovered code
(`JSR $A2BA` at `$9681` and `$9A57`).

## Oracle cross-check (3,000 frames)

`cargo xtask disasm ... --covered --frames 3000 --press 180:start` gives the
oracle's executed-PC set (2,456 distinct PCs). Comparing exact instruction
starts:

| | executed PCs covered | share |
|---|---:|---:|
| baseline Ghidra | 966 / 2,456 | 39.3% |
| after recovery | 2,454 / 2,456 | 99.9% |

Coverage delta: **+1,488 executed PCs (+60.6 points)**. In `$8C7D-$EC17` the
baseline covered 0 of 1,238 executed PCs; the recovery covers all 1,238.

Of the 167 new functions, **60 have their entry executed** in this route
(17 of the 66 pointer-entry functions and all 43 recovered direct callees). If
"executed" is read as "any instruction in the body", the count is 65 (22 pointer
+ 43 callees). The remainder are real handlers for states or enemy types that
the 3,000-frame stage-1 route never enters (title, other stages, other spawns).

## Using the map in `xtask`

The map ships as a stable symbol file so `xtask` can label its own listing
instead of only `Lxxxx` branch targets:

```sh
# Normalize the Ghidra export into tools/ghidra/symbols.txt (214 symbols).
cargo xtask symbols

# Label operands and function entries with the stable names.
cargo xtask disasm "Gradius (E).nes" \
  --symbols tools/ghidra/symbols.txt --labels --from a2ba --to a2f8

# Without --symbols, disasm/trace load tools/ghidra/functions_full.txt when it
# is present. --covered also marks executed instructions and their hit counts.
cargo xtask disasm "Gradius (E).nes" --covered --frames 3000 --press 180:start \
  --from a2ba --to a2f8
```

`symbols.txt` is `<address> <name> <size>` per line; `#`/`;` lines are
comments. `--merge` (default `functions_full.txt`) only appends addresses that
are not already in the output file, so hand-written names survive a re-merge.
The file now carries one-line stage-1 annotations (`stage1_spawner`,
`stage1_kind0f`, `stage1_big_core`, ...) on top of Ghidra's `FUN_PRG0__xxxx`
names; re-running `cargo xtask symbols` keeps them.

`trace --symbols FILE` annotates the hottest PCs (and `--log` lines) with the
same names. `ramdump` does not take symbols: its addresses are CPU RAM
(`$0000-$07FF`), not PRG.

### Stage-1 spawner excerpt

`$A2BA` is the stage-section spawner and `$A2CB` is its table-read entry
(the fallthrough the static pass initially missed). Regenerated with the
command above; `*` and the counts come from the 3,000-frame oracle run:

```text
stage1_spawner:
*$A2BA  LDA  $3A  ; 2624
*$A2BC  BEQ  LA2C1  ; 2624
$A2BE  JMP  FUN_PRG0__c40d
LA2C1:
*$A2C1  LDX  $60  ; 2624
*$A2C3  BNE  LA2C6  ; 2624
$A2C5  RTS
LA2C6:
*$A2C6  DEX  ; 2624
*$A2C7  BNE  LA2EA  ; 2624
*$A2C9  INC  $60  ; 4
LA2CB:
*$A2CB  LDA  $19  ; 4
*$A2CD  ASL  A  ; 4
*$A2CE  TAY  ; 4
*$A2CF  LDA  LA7CA,Y  ; 4
*$A2D2  STA  $98  ; 4
*$A2D4  LDA  LA7CB,Y  ; 4
*$A2D7  STA  $99  ; 4
```

`$A2CB` executes four times in this route; the number after `;` is the hit
count from the oracle. `LA7CA`/`LA7CB` are the low/high bytes of the `$A7CA`
stage-section pointer table.

## What is still missing

- `$889D/$889F` are the only executed PCs that are not instruction starts. A
  `JSR $889D` at `$9AC1` reaches a real routine, but `FUN_8876`'s conditional
  fallthrough at `$8896` runs into the five data bytes at `$8898`, so Ghidra
  keeps `$8898-$88B1` in that body. Splitting it needs a data/code boundary
  override on an existing function; it is not pointer-table related.
- `$899C` is a valid inline-table target (`LDA $46`) but is not executed in this
  route and remains shadowed by the same kind of over-long function body.
- Inline dispatch is no longer missing. A future table read through a computed
  `JMP ($zp)` with no inline block or named table would still be invisible; the
  oracle shows no such executed path here.

## Reproduce

```sh
GHIDRA_HOME=/tmp/opencode/ghidra/ghidra_12.1.4_PUBLIC
ROM="$GRADIUS_ROM"
PROJ=/tmp/opencode/ghidra-out/proj

"$GHIDRA_HOME/support/analyzeHeadless" "$PROJ" gradius \
  -import "$ROM" -overwrite \
  -scriptPath tools/ghidra -postScript RecoverCode.java /tmp/opencode/ghidra-out/recover

cargo xtask disasm "$ROM" --covered --frames 3000 --press 180:start \
  --out /tmp/opencode/ghidra-out/oracle.disasm
```
