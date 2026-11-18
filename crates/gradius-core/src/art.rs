//! Tile references used by the slice, measured from the original.
//!
//! `bank` is the index of a 4 KiB pattern-table half (see
//! `gradius_pack::TILE_BANKS`): `hardware_bank * 2 + table`, where `table`
//! is 0 for `$0000` and 1 for `$1000`. Sprites live in the `$1000` table, so
//! the values below use bank 1 (hardware CHR bank 0, sprite pattern table).
//!
//! All values were read with `cargo xtask probe` (see
//! `docs/analysis/stage1.md` for the exact commands, dumps and hashes).

use crate::assets::TileRef;

/// The Vic Viper, nose right.
///
/// The ship is three 8x16 sprites (`$08/$09`, `$0A/$0B`, `$0C/$0D`) at OAM
/// slots 34, 49 and 15, x = 72/80/88, stored Y = 96, attribute bytes `$21`,
/// `$20`, `$20`. `$08/$09` uses sprite palette 1, the other two use palette 0.
/// The single 8x8 tile below is the middle (`$0A`) of that set, the most
/// representative standalone piece for this port's one-tile player.
pub const PLAYER: TileRef = TileRef {
    bank: 1,
    index: 0x0A,
};

/// The three 8x16 columns that compose the ship, rear to nose (`$08`, `$0A`,
/// `$0C`; each is the top tile of a pair). Draw them 8 px apart at the same Y.
pub const PLAYER_PARTS: [TileRef; 3] = [
    TileRef {
        bank: 1,
        index: 0x08,
    },
    TileRef {
        bank: 1,
        index: 0x0A,
    },
    TileRef {
        bank: 1,
        index: 0x0C,
    },
];

/// Player shot.
///
/// Sprite group 8/9 of the table at `$8D92` (definition `$8F58`) is a single
/// 8x16 sprite with tile byte `$65` (effective `$64/$65`) and attribute `$22`
/// (sprite palette 2, in front). OAM slots 1, 16 and 31 carried it at frame
/// 800. The single 8x8 tile is the upper half, `$64`.
pub const BULLET: TileRef = TileRef {
    bank: 1,
    index: 0x64,
};

/// Small enemy, one of the fan-shaped waves.
///
/// 8x16 sprite `$C0/$C1` with attribute `$21` (sprite palette 1, behind the
/// background). Seen live at OAM slot 4, x = 95, stored Y = 95 (top 96), in
/// the frame-800 gameplay snapshot, where it overlaps the ship.
pub const ENEMY_SMALL: TileRef = TileRef {
    bank: 1,
    index: 0xC0,
};

/// Larger enemy.
///
/// Sprite group 0 of the table at `$8D92` (`$9193`): six 8x16 sprites in a
/// 2x3 block using tiles `$D1/$D3/$D5` with attributes `$21/$22`. The 8x8
/// value below is the upper half of the first tile (`$D0`).
pub const ENEMY_LARGE: TileRef = TileRef {
    bank: 1,
    index: 0xD0,
};

/// The three 8x16 columns of the large enemy, left to right.
///
/// The sprite-group definition stores the pair tile bytes `$D1/$D3/$D5`, so
/// the pairs are `$D0/$D1`, `$D2/$D3` and `$D4/$D5`. Drawn 8 px apart at the
/// same Y to reproduce the measured 24x16 block.
pub const ENEMY_LARGE_PARTS: [TileRef; 3] = [
    TileRef {
        bank: 1,
        index: 0xD0,
    },
    TileRef {
        bank: 1,
        index: 0xD2,
    },
    TileRef {
        bank: 1,
        index: 0xD4,
    },
];

/// The secondary missile in free flight.
///
/// `$A265` sets sprite frame `$0A`, which `$8AB1` resolves to `$8F58`: one
/// 8x16 sprite with OAM tile byte `$65` (the pair `$64/$65`) and attribute
/// `$22`. The ground-ride frame `$08` resolves to `$8F53`, tile byte `$61`
/// (pair `$60/$61`); [`crate::game`] draws it through that group. `TileRef`
/// stores the upper half of the pair, so `$65` becomes `$64` here.
pub const MISSILE: TileRef = TileRef {
    bank: 1,
    index: 0x64,
};

/// HUD power-meter bar.
///
/// Sprite group `$8F4A`: the two solid rows drawn as an 8x16 sprite pair
/// `$6C/$6D` at OAM slot 0, x = 248, stored Y = 206, attribute `$23`.
pub const HUD_BAR: TileRef = TileRef {
    bank: 1,
    index: 0x6C,
};

/// The first explosion/debris pair (`$58/$59`), sprite palette 3.
pub const EXPLOSION_A: TileRef = TileRef {
    bank: 1,
    index: 0x58,
};

/// The second explosion/spark pair (`$6A/$6B`), sprite palette 2.
pub const EXPLOSION_B: TileRef = TileRef {
    bank: 1,
    index: 0x6A,
};

/// An option drone.
///
/// The `$68/$69` pair draws a small round orb in its upper half, matching the
/// size of the original's option sprites. Also read from the sheet; not
/// captured in a live frame.
pub const OPTION: TileRef = TileRef {
    bank: 1,
    index: 0x68,
};
