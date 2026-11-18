use super::render::*;
use super::*;
use crate::assets::StageAssets;

// The tests are split by area. The area files are `include!`d into this module
// instead of being declared as submodules, so every test keeps the flat
// `game::tests::<name>` path the harness and the oracle lists use.
include!("tests/common.rs");
include!("tests/save.rs");
include!("tests/player.rs");
include!("tests/meter.rs");
include!("tests/lifecycle.rs");
include!("tests/objects.rs");
include!("tests/bosses.rs");
include!("tests/collision.rs");
include!("tests/spawner.rs");
include!("tests/stage_flow.rs");
include!("tests/ending.rs");
include!("tests/title.rs");
include!("tests/render.rs");
include!("tests/simul.rs");
include!("tests/pause.rs");
