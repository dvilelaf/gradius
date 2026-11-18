# Stage 2-6 collision maps: nametable cross-check

Measured 2026-11-11 on `agent/collverify` against `Gradius (E).nes`. The ROM was
read in place; no cartridge bytes are stored here.

## What is compared

`stage_collision::map(section)` holds the packed `$0500`/`$0600` bytes that
`$C3CD` reads: one byte per 8-pixel column per four tile rows, one 2-bit field
per row (`$C409` masks `03 0C 30 C0`). The maps were extracted by replaying each
stage in the oracle and keeping the first value seen for every
`(column, group)`.

The independent oracle is the displayed nametable: `stage2_bg::STAGE2` and
`stage_bgs::STAGE3..STAGE6` are the reconstructed 28-row background and its
attribute table, already validated against the live nametable at several
scroll positions by `xtask screens --sample-scroll`. For a world tile column
`c` and row `r`, the attribute quadrant is
`(attributes[(r/4)*width/4 + c/4] >> (2*((r%4)/2*2 + (c%4)/2))) & 3`.

Palettes 0/1 are the starfield/backdrop in every stage; palettes 2/3 are the
structural fill. The check is therefore one-way: **every extracted solid cell
must select palette 2 or 3 in the nametable**. The converse is not required:
palette 2/3 also paints decorative, non-solid art, so coverage below 100% is
expected.

## Agreement

Columns 0..447 were compared (each phase's background may be shorter; phase 6
ends at column 415). "Solid" is an extracted 2-bit field different from zero.
"Backed" is a solid cell whose attribute quadrant is 2 or 3.

| phase | solid cells | backed | agreement | attr. structure cells | coverage |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 1213 | 1213 | **100.0%** | 1856 | 65.4% |
| 3 | 1059 | 1059 | **100.0%** | 4648 | 22.8% |
| 4 | 1339 | 1339 | **100.0%** | 2220 | 60.3% |
| 5 | 627 | 0 | **0.0%** | 112 | 0.0% |
| 6 | 2534 | 2534 | **100.0%** | 3240 | 78.2% |

Sample blocks every 32 columns (extracted solid / backed):

| phase | 0-3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 0/0 | 145/145 | 122/122 | 179/179 | 183/183 | 145/145 | 122/122 | 317/317 | 0/0 | 0/0 | 0/0 |
| 3 | 0/0 | 81/81 | 126/126 | 117/117 | 156/156 | 131/131 | 117/117 | 156/156 | 175/175 | 0/0 | 0/0 |
| 4 | 0/0 | 145/145 | 139/139 | 156/156 | 132/132 | 163/163 | 132/132 | 132/132 | 178/178 | 162/162 | 0/0 |
| 5 | 0/0 | 0/0 | 52/**0** | 0/0 | 194/**0** | 0/0 | 191/**0** | 0/0 | 190/**0** | 0/0 | 0/0 |
| 6 | 0/0 | 201/201 | 282/282 | 511/511 | 268/268 | 340/340 | 333/333 | 303/303 | 296/296 | 0/0 | 0/0 |

## Conclusion per phase

* **Phase 2**: all 1213 solid cells sit on palette-2 structure; no solid cell
  contradicts the nametable. The map is consistent.
* **Phase 3**: all 1059 solid cells sit on palette-3 structure. Coverage is
  low because phase 3 paints large palette-3 areas that are not collidable;
  the one-way agreement is still exact.
* **Phase 4**: all 1339 solid cells are backed, including the 162 cells in
  block 12 (columns 384-415).
* **Phase 5**: **not consistent**. See below.
* **Phase 6**: all 2534 solid cells are backed (palettes 2 and 3). The
  background stops at column 415, so columns 416-447 (blocks 13) have no
  nametable reference; the extracted map is zero there.

## Phase 5: the sparse map is an extraction defect

The oracle's stage-5 nametable is a uniform starfield: all fourteen 32-column
blocks have exactly the same attribute pattern
(`{palette 0: 616, palette 1: 272, palette 3: 8}` cells per block) and there is
no structural block anywhere in the 448 columns. The checked-in map, however,
marks 627 solid cells, all in the odd blocks 5, 7, 9 and 11:

* columns 160-191 (block 5): 52 solid cells;
* columns 224-255 (block 7): 194 solid cells;
* columns 288-319 (block 9): 191 solid cells;
* columns 352-383 (block 11): 190 solid cells.

Blocks 7, 9 and 11 put a non-zero byte on **exactly the same 59 cells**; only
their values differ, and every even block is empty. A genuine terrain layout
does not repeat its support three times. This is the recycled `$0500`/`$0600`
page being sampled before it was reloaded, and keeping the first value
preserved the old page. None of the 627 cells has nametable backing
(agreement 0%). The phase-5 map should be all zero within the visible 448
columns; the current map gives the port phantom terrain in the middle of the
stage. The extraction must be redone with a capture policy that commits a page
only after its 32 columns have been reloaded (or cross-checks each page against
the nametable, as done here).

First exact contradiction: **world column 160** (block 5), rows 8-11
(`$0500` group 2 = `0x50`), where the nametable is plain starfield.

## Tramos finales

There is no residue in the final sections of phases 2-6:

* blocks 12-13 (columns 384-447) are zero in phases 2, 3, 5 and 6;
* phase 4 has real terrain in block 12 (162 solid cells, all backed) and zero
  in block 13;
* phase 6's columns 416-447 are past the last displayed nametable column, but
  the extracted map is zero there, so no recycled data leaked in.

The worker's suspicion about the tail is therefore not confirmed for these
maps: the only residue found is phase 5's middle blocks.

## Reproduce

The check is in
`crates/gradius-diff/tests/stage_collision_maps.rs` and needs no cartridge.
The nametable dumps can be regenerated with:

```sh
cargo xtask screens "$GRADIUS_ROM" --frames 11000 --assist \
    --press 180:start --stage 2 --dump /tmp/stage2.grd
```

`xtask screens` also reports `0 tile / 0 attribute mismatches` against the
live nametable at the sampled scrolls for stages 2, 5 and 6.
