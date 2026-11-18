//! Object AI: the `$AE16` jump table targets and their private helpers.
//!
//! Moved verbatim from `game.rs` (card R1a) and split by family in card
//! R1e; every handler keeps its name, its body and its place in the
//! `update_objects` dispatch.

use super::*;

// `game.rs` declares this module with `#[path = "ai.rs"]`, so the family files
// under `ai/` carry their own paths (card R1e) instead of the usual `mod x;`.
#[path = "ai/common.rs"]
mod common;
#[path = "ai/large.rs"]
mod large;
#[path = "ai/turrets.rs"]
mod turrets;
#[path = "ai/walkers.rs"]
mod walkers;

#[cfg(test)]
mod tests {
    use super::*;

    /// A playing game with an empty object table and the ship at `x`.
    fn core_game(player_x: u8) -> Game {
        let assets = Assets {
            tiles: vec![[0u8; 64]; 8 * 256],
            colors: (0..64).map(|_| [0xFF, 0xFF, 0xFF]).collect(),
            rom: Some(crate::rom::testing::rom()),
            ..Default::default()
        };
        let mut game = Game::new(assets);
        game.state = State::Playing;
        game.new_run();
        game.phase = Phase::Normal;
        game.objects = [Object::default(); SLOT_COUNT];
        game.player = Player::new(player_x, 0x60);
        game
    }

    fn core(x: u8, ai_state: u8) -> Object {
        Object {
            active: true,
            kind: 0x98,
            x: Fixed::px(x),
            y: Fixed::px(0x50),
            sprite: 1,
            boss: true,
            ai_state,
            ..Object::default()
        }
    }

    #[test]
    fn the_core_entry_falls_through_when_the_damage_gate_is_open() {
        // `$B9A2`-`$B9AF`: x >= $A4 spends the entry pixel, then the open
        // `$048C` gate BNE into `$BA04`: the frame still tracks (+2 here),
        // re-aims, moves vertically and advances the `$BA6A` counter.
        let mut open = core_game(0xA4);
        let mut object = core(0xA5, 1);
        open.ai_big_core(&mut object);
        assert_eq!(object.x.int, 0xA6, "-1 entry then +2 track");
        assert_eq!(object.timer2, 1, "the fight path's counter ran");

        // `$048C == 0` takes the `$B9B1` rebuild-and-RTS path instead.
        let mut closed = core_game(0xA4);
        let mut object = core(0xA5, 0);
        closed.ai_big_core(&mut object);
        assert_eq!(object.x.int, 0xA4, "entry only");
        assert_eq!(object.timer2, 0, "no fight path");
    }
}
