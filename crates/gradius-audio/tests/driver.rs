use gradius_audio::driver::{Driver, Part, RegWrite};

fn bank(program: &[u8]) -> Vec<u8> {
    let mut mem = vec![0u8; 0x100];
    mem[..program.len()].copy_from_slice(program);
    mem
}

fn frame(driver: &mut Driver<'_>) -> Vec<(u16, u8)> {
    let mut writes: Vec<RegWrite> = Vec::new();
    driver.tick(&mut writes);
    writes.into_iter().map(|w| (w.addr, w.value)).collect()
}

#[test]
fn parses_a_dx_note_and_advances_the_pointer() {
    // $D2 sets tempo 2 and config $7F; $E3 $10 is octave 3, note C# with
    // duration nibble 0. $FF ends the channel on the next parse.
    let mem = bank(&[0xD2, 0x7F, 0x00, 0xE3, 0x10, 0xFF]);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 0,
            ptr: 0x8000,
        }],
    );

    assert_eq!(
        frame(&mut driver),
        vec![(0x4000, 0x7F), (0x4003, 0x0B), (0x4002, 0x27),]
    );
    assert_eq!(frame(&mut driver), vec![]);
    assert_eq!(frame(&mut driver), vec![(0x4000, 0x30)]);
}

#[test]
fn tempo_sets_the_frame_spacing_between_notes() {
    // Tempo 3: duration nibble 0 lasts 3 frames, nibble 1 lasts 6, nibble 2
    // lasts 9. Each note rewrites $4000 with the same $7F config.
    let mem = bank(&[0xD3, 0x7F, 0x00, 0xE0, 0x70, 0x71, 0x72, 0xFF]);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 0,
            ptr: 0x8000,
        }],
    );

    let mut starts = Vec::new();
    for f in 0..20 {
        if frame(&mut driver).contains(&(0x4000, 0x7F)) {
            starts.push(f);
        }
    }
    assert_eq!(starts, vec![0, 3, 9]);
}

#[test]
fn envelope_fades_one_volume_step_per_frame() {
    // Config $7F, fade parameters $50 (five-frame lead-in, step 0) and a note
    // lasting seven frames. The fade rewrites $4000 from $7E downwards.
    let mem = bank(&[0xD7, 0x7F, 0x50, 0xE1, 0x70, 0xFF]);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 0,
            ptr: 0x8000,
        }],
    );

    assert_eq!(frame(&mut driver)[0], (0x4000, 0x7F));
    for (frame_number, expected) in [(1, 0x7E), (2, 0x7D), (3, 0x7C)] {
        let write = frame(&mut driver)
            .into_iter()
            .find(|(addr, _)| *addr == 0x4000)
            .unwrap();
        assert_eq!(write, (0x4000, expected), "frame {frame_number}");
    }
}

#[test]
fn a_ff_repeat_resets_instead_of_counting_to_the_end() {
    // $FE $FF loops back to the note after each frame. The 6502 keeps the
    // counter at zero because `$01 - $FF` has bit 7 clear, so a naive
    // incrementing counter would wrongly reach $FF and fall through to the
    // trailing $FF after 255 passes.
    let mem = bank(&[0xD1, 0x7F, 0x00, 0xE4, 0x70, 0xFE, 0xFF, 0x00, 0x80, 0xFF]);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 0,
            ptr: 0x8000,
        }],
    );

    for frame_number in 0..400 {
        let writes = frame(&mut driver);
        assert!(
            !writes.contains(&(0x4000, 0x30)),
            "channel ended at frame {frame_number}"
        );
        assert!(
            writes.contains(&(0x4000, 0x7F)),
            "note did not start at frame {frame_number}"
        );
    }
}

#[test]
fn tempo_scaling_lowers_the_pulse_two_volume_every_48_frames() {
    // Tempo 2 and three notes of duration 26 frames. The scaling counter steps
    // at frame 47, so the third note's low nibble is one lower than the first.
    let mem = bank(&[0xD2, 0x7F, 0x00, 0xE1, 0x7C, 0x7C, 0x7C, 0xFF]);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 1,
            ptr: 0x8000,
        }],
    );
    driver.set_tempo_scaling(true, 0x10);

    let mut volumes = Vec::new();
    for frame_number in 0..60 {
        let writes = frame(&mut driver);
        if let Some(&(_, value)) = writes.iter().find(|(addr, _)| *addr == 0x4004) {
            if matches!(frame_number, 0 | 26 | 52) {
                volumes.push((frame_number, value));
            }
        }
    }
    assert_eq!(volumes, vec![(0, 0x7F), (26, 0x7F), (52, 0x7E)]);
}

#[test]
fn tempo_scaling_silences_the_triangle_once_the_limit_drops_below_seven() {
    // Pulse 2 config $77 drives the limit down to 6 by frame 52; the next
    // 48-frame counter tick (frame 95) then frees the triangle and writes
    // silence to $4008/$4009, after which the triangle stays quiet.
    let pulse2 = [0xD2, 0x77, 0x00, 0x0C, 0x0C, 0x0C, 0x0C, 0xFF];
    let triangle = [0xD1, 0x0F, 0x0C, 0x0C, 0x0C, 0x0C, 0xFE, 0xFF, 0x22, 0x80];
    let mut mem = bank(&[0u8; 0x40]);
    mem[0x00..pulse2.len()].copy_from_slice(&pulse2);
    mem[0x20..0x20 + triangle.len()].copy_from_slice(&triangle);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x10,
        &[
            Part {
                channel: 1,
                ptr: 0x8000,
            },
            Part {
                channel: 2,
                ptr: 0x8020,
            },
        ],
    );
    driver.set_tempo_scaling(true, 0x10);

    let mut silence = None;
    for frame_number in 0..120 {
        let writes = frame(&mut driver);
        if writes.contains(&(0x4008, 0x00)) && writes.contains(&(0x4009, 0x00)) {
            silence = Some(frame_number);
            break;
        }
    }
    assert_eq!(silence, Some(95));
    for frame_number in 96..120 {
        let writes = frame(&mut driver);
        assert!(
            !writes
                .iter()
                .any(|(addr, _)| (0x4008..=0x400B).contains(addr)),
            "triangle wrote again at frame {frame_number}"
        );
    }
}

#[test]
fn out_of_range_f0_fc_notes_read_the_period_after_the_table() {
    // A pitch nibble of $F indexes the two bytes after the twelve-entry period
    // table, which fall inside the request table (the start of entry $03).
    let mut mem = vec![0u8; 0x4000];
    let program = [0xD1, 0x7F, 0x00, 0xE4, 0xF1, 0xF1, 0xFF];
    mem[0x100..0x100 + program.len()].copy_from_slice(&program);
    let alias = 0xEFD0usize - 0xC000;
    mem[alias..alias + 2].copy_from_slice(&[0x0A, 0x5C]);
    let mut driver = Driver::new(0xC000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 0,
            ptr: 0xC100,
        }],
    );

    assert_eq!(
        frame(&mut driver),
        vec![(0x4000, 0x7F), (0x4003, 0x0A), (0x4002, 0x5C)]
    );
}

#[test]
fn fc_uses_the_duration_nibble_for_an_alias_note() {
    // $FC is pitch $F with duration nibble $C; with tempo 1 it holds for 13
    // frames before the following alias note starts.
    let mut mem = vec![0u8; 0x4000];
    let program = [0xD1, 0x7F, 0x00, 0xE4, 0xFC, 0xF1, 0xFF];
    mem[0x100..0x100 + program.len()].copy_from_slice(&program);
    let alias = 0xEFD0usize - 0xC000;
    mem[alias..alias + 2].copy_from_slice(&[0x0A, 0x5C]);
    let mut driver = Driver::new(0xC000, &mem);
    let _ = driver.start(
        0x10,
        &[Part {
            channel: 0,
            ptr: 0xC100,
        }],
    );

    let mut starts = Vec::new();
    for frame_number in 0..20 {
        if frame(&mut driver).contains(&(0x4002, 0x5C)) {
            starts.push(frame_number);
        }
    }
    assert_eq!(starts, vec![0, 13]);
}

#[test]
fn parses_the_sound_effect_path() {
    // $23 sets duration 3 and volume, $10 writes sweep $83, the volume bit 4
    // pulls the high nibble of $F0 into $4000, then a raw period $20.
    let mem = bank(&[0x23, 0x7F, 0x10, 0x83, 0xF0, 0x20, 0xFF]);
    let mut driver = Driver::new(0x8000, &mem);
    let _ = driver.start(
        0x01,
        &[Part {
            channel: 0,
            ptr: 0x8000,
        }],
    );

    assert_eq!(
        frame(&mut driver),
        vec![
            (0x4000, 0x7F),
            (0x4001, 0x83),
            (0x4000, 0x7F),
            (0x4003, 0x08),
            (0x4002, 0x20),
        ]
    );
}
