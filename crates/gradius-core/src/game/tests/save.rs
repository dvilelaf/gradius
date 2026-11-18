#[test]
fn bcd_score_adds_and_saturates() {
    let mut score = Bcd::default();
    score.add(100);
    assert_eq!(score.digits, [0x00, 0x01, 0x00]);
    assert_eq!(score.value(), 100);
    score.add(995);
    assert_eq!(score.digits, [0x00, 0x10, 0x95]);
    assert_eq!(score.value(), 1095);
    score.add(999_000);
    assert_eq!(score.digits, [0x99, 0x99, 0x99]);
    assert_eq!(score.value(), 999_999);
}

#[test]
fn bcd_score_never_gets_a_non_decimal_nibble() {
    let mut score = Bcd::default();
    for _ in 0..10_000 {
        score.add(777);
    }
    for byte in score.digits {
        assert!(byte >> 4 <= 9);
        assert!(byte & 0x0F <= 9);
    }
}

#[test]
fn the_cartridge_seeds_the_high_score_with_0050000() {
    // `$8052`: cold boot writes `$50` into `$07E1`.
    let game = playing();
    assert_eq!(game.high_score(), 5_000);
    assert_eq!(game.high_score_bcd(), [0x00, 0x50, 0x00]);
}

#[test]
fn the_high_score_follows_the_score_only_when_it_passes_it() {
    // `$84FC-$8514`: compare from the most significant BCD byte down and
    // copy the score only when it is larger, so a lower score or a tie
    // leaves the high score alone.
    let mut game = playing();
    game.add_score(4_999);
    assert_eq!(game.high_score(), 5_000, "4999 is below the seed");
    game.add_score(1);
    assert_eq!(game.high_score(), 5_000, "a tie keeps the seed");
    game.add_score(5_000);
    assert_eq!(game.score, 10_000);
    assert_eq!(game.high_score(), 10_000, "10000 replaces 5000");
    assert_eq!(game.high_score_bcd(), [0x01, 0x00, 0x00]);
}

#[test]
fn a_new_run_clears_the_score_but_never_lowers_the_high_score() {
    let mut game = playing();
    game.add_score(12_345);
    assert_eq!(game.high_score(), 12_345);
    game.new_run();
    assert_eq!(game.score, 0);
    assert_eq!(game.high_score(), 12_345, "the high score survives");
}
