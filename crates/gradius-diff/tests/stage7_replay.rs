//! Replay the checked-in stage-7 fixture against the native port.
//!
//! This test needs no cartridge, only the extracted asset pack. The fixture covers the
//! poked stage-7 window, oracle frames 297-6050: `--stage 7` pokes `$26`, so
//! the oracle enters stage 7 (`$19 = 6`) at 265 and normal play at 292; the
//! native `Game::new_at_stage` starts five frames later at 297
//! (`STAGE_BOOT_LEAD`), the first frame the harness pairs. The window runs
//! through the stage's single scroll freeze in the oracle: `$3F = $0B` holds
//! the scroll still from frame 5929 through 6050, so the last recorded frame is
//! the last frozen frame and the resumed scroll at 6051 is outside the window.
//! `docs/analysis/stage67_boot.md` has the full stage-7 timeline: the clear
//! window opens at 6563, stage 7 ends at 7024 and the ending (`$87`..`$8D`)
//! runs to 8254, all beyond this fixture.
//!
//! Regenerate the fixture from a cartridge with:
//!
//! ```sh
//! ./target/release/gradius-diff --rom "$ROM" \
//!   --emit-fixture tests/fixtures/stage7-297-6050.state \
//!   --stage 7 --frames 6050 --from 297 --to 6050 --keep-alive
//! ```
//!
//! `--keep-alive` pokes the oracle and so is part of the fixture; the replay
//! also enables the port's `Game::dev_keep_alive(true)`, or a native death
//! would freeze scroll and lives while the kept-alive oracle plays on.
//!
//! The `--frames` value matters: the default script is generated from the run
//! length, so the recording must use the length this test builds with
//! [`RECORDED_FRAMES`]. The oracle's frame counter is 1-based, so `--frames
//! 6050` records frames 1-6050 and the window ends on the run's last frame.
//!
//! This test **gates** the window: it ends at 6050, before the clear
//! window opens at 6563, and the native scroll now reaches the oracle's
//! 5929-6050 `$3F = $0B` freeze. The replay is a gate: the window must replay
//! with no divergence, and the structural assertions pin the shape of the
//! comparison. The clear (6563) and the ending are covered by `ending_replay`.
//!
//! What is wired: `Game::new_at_stage` runs the three-frame `$9B38` warm-up
//! that stages 4-6 also run, and the core reads `QUIRK_NO_CORE`/`QUIRK_LOOP`
//! at the stage's `$3F = $0C` trigger (`$99B3`). Stage 7 takes the no-core
//! `$81`/`$82`/`$83`/`$86` sequence with `$9A01`'s fixed `$4D = 1, $4C = $CA`
//! timer (458 frames); on the `$86` frame `$98F7` jumps to `$9865`, which
//! increments `loop_count` and clears `$3F`.
//!
//! Beyond this fixture: the `$87`..`$8D` ending is implemented and gated
//! separately by `ending_replay` (oracle 7025-8290), and the stage-1 loop
//! replay after it is not covered here. The fixture now reports 604
//! divergences, the first at 5627 `obj.y`; `scroll` is byte-identical over the
//! whole window, 5754/5754. Seeding `nmi_frame` from the cartridge at the
//! `--stage` anchor removed the kind-`$90` turret's 9-frame-early diver shells
//! and the ~80 % cascade they caused, which `docs/analysis/stage7_triage.md`
//! groups by cause. The remaining 604 are all the independent kind-`$93`
//! ceiling-walker `obj.y` group (frames 5627-5863). The `$B563` wall
//! (`ai_wall`) now freezes it at 5929-6050 like the oracle: the old handler
//! compared the wall's X the wrong way and froze 64 px early at 5801.
//!
//! The report-only status is the residue itself, not a missing ending: the
//! remainder is the kind-`$93` ceiling walker. Once that is fixed, this replay
//! takes the exact gate the stage-4/5/6 replays have and
//! `docs/KNOWN_DEVIATIONS.md` drops the report-only row.
//!
//! The pack test at the bottom checks what the ending needs against
//! `assets/gradius.pak` through the `gradius-assets` API and reports the
//! missing pieces; it is skipped when the generated pack is absent.

use std::path::PathBuf;

use gradius_assets::{ENDING_TEXT_RECORD, STAGE_WALL_ERASE};
use gradius_core::Game;
use gradius_diff::compare::{compare, CompareConfig, Report};
use gradius_diff::fixture::read_fixture;
use gradius_diff::script::default_script;
use gradius_diff::testdata;
use gradius_diff::{core_buttons, parse_fixture, Snapshot};

/// The oracle frame the harness pairs with native frame 1.
const FIRST_FRAME: u64 = 297;
/// The last compared frame: the last frame of the `$3F = $0B` scroll freeze.
const LAST_FRAME: u64 = 6050;
/// The stage-7 zero-page section (`$19 = 6`).
const STAGE_INDEX: u8 = 6;
/// Native frame 1 is the oracle's frame 297.
const FRAME_OFFSET: u64 = FIRST_FRAME - 1;
/// The recording was made with `--frames 6050`; the default script is a
/// function of the run length, so the test must build the same one. The
/// oracle's frame counter is 1-based, so this is also [`LAST_FRAME`].
const RECORDED_FRAMES: u64 = LAST_FRAME;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Replay the recorded window and compare it over the full 5754 frames.
fn replay() -> Report {
    let fixture_path = root().join("tests/fixtures/stage7-297-6050.state");
    let text = read_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let oracle = parse_fixture(&text).expect("malformed fixture");
    let script = default_script(RECORDED_FRAMES);

    let mut game = Game::new_at_stage_seeded(testdata::assets(), STAGE_INDEX, 0x24 - 1);
    // The diagnostic run uses `--native-keep-alive`: without it a native death
    // freezes scroll and lives while the kept-alive oracle plays on.
    game.dev_keep_alive(true);
    let mut native: Vec<Snapshot> = Vec::with_capacity(oracle.len());
    for snapshot in &oracle {
        // The harness loop is 0-based and steps the oracle before sampling it,
        // so oracle frame N was run with the buttons of harness frame N - 1.
        game.step(core_buttons(script.buttons(snapshot.frame - 1)));
        let mut native_snapshot = Snapshot::parse(&game.dump_state()).expect("native state line");
        native_snapshot.frame += FRAME_OFFSET;
        native.push(native_snapshot);
    }

    let pairs: Vec<(&Snapshot, &Snapshot)> = oracle.iter().zip(&native).collect();
    compare(
        pairs,
        CompareConfig {
            window: Some((FIRST_FRAME, LAST_FRAME)),
            ..CompareConfig::default()
        },
    )
}

/// Gate the checked-in window: 297-6050 must replay with no divergence.
///
/// The window stops at 6050, before the clear opens at 6563 and before the
/// ending (`$87`..`$8D`) at 7024; those are covered by `ending_replay`.
#[test]
fn stage7_replay_is_exact() {
    let report = replay();
    assert_eq!(report.frames, LAST_FRAME - FIRST_FRAME + 1);
    assert_eq!(report.compared, report.frames);
    assert_eq!(report.requested_window, Some((FIRST_FRAME, LAST_FRAME)));
    // The window must exercise the simulation, not just a static frame.
    for field in ["px", "py", "scroll", "score", "obj.kind", "obj.x", "obj.y"] {
        assert!(
            report.fields[field].checks > 0,
            "the fixture does not exercise {field}"
        );
    }
    assert!(report.object_slot_pairs > 0, "no object slots compared");

    eprintln!("stage-7 fixture replay:");
    eprintln!("{}", report.summary());
    match report.first() {
        Some(first) => eprintln!(
            "first divergence: frame {} {} (oracle {}, native {})",
            first.frame, first.field, first.oracle, first.native
        ),
        None => eprintln!("first divergence: none"),
    }
    assert!(report.within_tolerance(), "{}", report.summary());
}

/// What the ending needs is in the generated pack, through the
/// `gradius-assets` API only.
///
/// * tiles: every CHR bank is packed (2048 tiles), so the font, starfield and
///   ending-ship sprite tiles are all there as pixels; the pack carries no
///   map from an object kind (`$28`) to its tiles, that table lives in PRG.
/// * palettes: the 64 NES colours are packed, enough for the ending's black
///   backdrop and star colours.
/// * audio: the GAUD chunk is captured, and `$A5` is the existing boss track,
///   so the ending music is data already in the pack.
/// * `$CF27`: the eight pointers and the 19-byte text record, ending on `$FE`.
/// * text/font and the starfield *parameters*: no chunk exists for either.
///   The port draws text with its built-in 3x5 font (`font::draw_text_*`) and
///   the starfield is code; the `CONGRATULATIONS!` string is not in the pack
///   (nor as ASCII in the ROM).
#[test]
fn ending_data_is_reported_from_the_pack() {
    let pack_path = root().join("assets/gradius.pak");
    if !pack_path.is_file() {
        eprintln!(
            "{} missing (run `cargo xtask extract`); skipping ending pack data check",
            pack_path.display()
        );
        return;
    }
    let pack = gradius_assets::Pack::load(&pack_path).expect("load the asset pack");

    // All four banks per pattern table: table, sprites and both pattern tables.
    assert_eq!(
        pack.tiles.len(),
        8 * 256,
        "the pack must carry every CHR bank for the ending sprites and font"
    );
    assert_eq!(pack.colors.len(), 64, "the pack must carry all NES colours");
    assert!(
        pack.audio.is_some(),
        "the pack must carry the GAUD audio chunk"
    );

    let stage = pack.stage.as_ref().expect("the pack must carry stage data");
    assert!(
        stage.table(STAGE_WALL_ERASE).is_some(),
        "the stage-6 wall-erase table is missing"
    );
    let table = stage
        .table(ENDING_TEXT_RECORD)
        .expect("the $CF27 ending text record is missing");
    assert_eq!(
        table.address, 0xCF27,
        "wrong ROM address for the ending text record"
    );

    let ending = pack
        .stage
        .as_ref()
        .and_then(|stage| stage.ending_text_record())
        .expect("the $CF27 record must decode as pointers plus text");
    assert_eq!(ending.pointers.len(), 8);
    assert_eq!(ending.text.len(), 19, "$CF35..$CF47 plus the $FE end");
    assert_eq!(
        *ending.text.last().unwrap(),
        0xFE,
        "the text record must end on $FE"
    );

    eprintln!("ending data in {}:", pack_path.display());
    eprintln!(
        "  tiles: {} (all CHR banks); palettes: {}; audio: GAUD {} bytes",
        pack.tiles.len(),
        pack.colors.len(),
        pack.audio.as_ref().map_or(0, Vec::len)
    );
    eprintln!(
        "  stage tables: {}; $CF27 record {} bytes ending ${:02X}",
        stage
            .tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        ending.text.len(),
        ending.text.last().unwrap()
    );
    eprintln!(
        "  missing: text/font (built-in 3x5 font, no chunk) and starfield \
         parameters (code); no object-kind -> sprite-tile mapping either"
    );
}
