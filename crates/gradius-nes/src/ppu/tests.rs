//! PPU tests.

use super::*;
use gradius_cart::{Cartridge, Header, NES_MASTER_PALETTE};

use super::render::{increment_coarse_x, increment_y};

fn cart_with(mirroring: Mirroring, chr: Vec<u8>) -> Cartridge {
    Cartridge {
        header: Header {
            prg_units: 2,
            chr_units: 4,
            mapper: 3,
            mirroring,
            has_trainer: false,
            has_battery: false,
        },
        prg: vec![0; 32 * 1024],
        chr,
        trainer: None,
        trailing: Vec::new(),
    }
}

fn cart(mirroring: Mirroring) -> Cartridge {
    cart_with(mirroring, vec![0; 32 * 1024])
}

fn set_addr(ppu: &mut Ppu, addr: u16) {
    ppu.cpu_write(0x2006, (addr >> 8) as u8);
    ppu.cpu_write(0x2006, addr as u8);
}

fn write_vram(ppu: &mut Ppu, addr: u16, value: u8) {
    set_addr(ppu, addr);
    ppu.cpu_write(0x2007, value);
}

fn read_vram(ppu: &mut Ppu, addr: u16) -> u8 {
    set_addr(ppu, addr);
    let _ = ppu.cpu_read(0x2007);
    ppu.cpu_read(0x2007)
}

fn read_palette_at(ppu: &mut Ppu, addr: u16) -> u8 {
    set_addr(ppu, addr);
    ppu.cpu_read(0x2007)
}

/// Write a tile's two bitplanes into a CHR bank.
fn set_tile(chr: &mut [u8], bank: usize, tile: u16, low: [u8; 8], high: [u8; 8]) {
    let base = bank * 0x2000 + usize::from(tile) * 16;
    chr[base..base + 8].copy_from_slice(&low);
    chr[base + 8..base + 16].copy_from_slice(&high);
}

fn solid(value: u8) -> ([u8; 8], [u8; 8]) {
    let low = if value & 1 != 0 { [0xFF; 8] } else { [0; 8] };
    let high = if value & 2 != 0 { [0xFF; 8] } else { [0; 8] };
    (low, high)
}

fn pixel(framebuffer: &[u8], x: usize, y: usize) -> [u8; 3] {
    let offset = (y * WIDTH + x) * 4;
    [
        framebuffer[offset],
        framebuffer[offset + 1],
        framebuffer[offset + 2],
    ]
}

fn color(index: u8) -> [u8; 3] {
    NES_MASTER_PALETTE[usize::from(index & 0x3F)]
}

#[test]
fn status_bits_clear_at_end_of_vblank() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    ppu.start_vblank();
    ppu.status |= 0x40 | 0x20;
    assert_ne!(ppu.status & 0x80, 0);
    ppu.end_vblank();
    assert_eq!(ppu.status & 0xE0, 0);
}

#[test]
fn fnv1a_matches_known_vector() {
    assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
}

#[test]
fn palette_mirrors_universal_background() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    write_vram(&mut ppu, 0x3F10, 0x21);
    assert_eq!(read_palette_at(&mut ppu, 0x3F00), 0x21);
    write_vram(&mut ppu, 0x3F00, 0x30);
    assert_eq!(read_palette_at(&mut ppu, 0x3F10), 0x30);
    write_vram(&mut ppu, 0x3F14, 0x22);
    assert_eq!(read_palette_at(&mut ppu, 0x3F04), 0x22);
}

#[test]
fn vertical_mirroring_maps_second_table_to_first() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    write_vram(&mut ppu, 0x2000, 0xAA);
    assert_eq!(read_vram(&mut ppu, 0x2800), 0xAA);
    write_vram(&mut ppu, 0x2400, 0xBB);
    assert_ne!(read_vram(&mut ppu, 0x2800), 0xBB);
}

#[test]
fn horizontal_mirroring_maps_lower_table_to_upper() {
    let mut ppu = Ppu::new(&cart(Mirroring::Horizontal));
    write_vram(&mut ppu, 0x2000, 0xAA);
    assert_eq!(read_vram(&mut ppu, 0x2400), 0xAA);
    write_vram(&mut ppu, 0x2800, 0xBB);
    assert_eq!(read_vram(&mut ppu, 0x2000), 0xAA);
    assert_eq!(read_vram(&mut ppu, 0x2800), 0xBB);
}

#[test]
fn ppuaddr_autoincrements() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    set_addr(&mut ppu, 0x2000);
    ppu.cpu_write(0x2007, 0x11);
    ppu.cpu_write(0x2007, 0x22);
    assert_eq!(ppu.vram[0], 0x11);
    assert_eq!(ppu.vram[1], 0x22);
}

#[test]
fn ppuaddr_increments_by_32_when_ctrl_bit2_is_set() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    ppu.cpu_write(0x2000, 0x04);
    set_addr(&mut ppu, 0x2000);
    ppu.cpu_write(0x2007, 0x11);
    assert_eq!(ppu.v, 0x2020);
}

#[test]
fn pattern_reads_follow_chr_bank_and_writes_are_ignored() {
    let mut chr = vec![0; 32 * 1024];
    chr[0] = 0xAA;
    chr[0x2000] = 0xBB;
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));

    set_addr(&mut ppu, 0x0000);
    ppu.cpu_write(0x2007, 0x55);
    assert_eq!(read_vram(&mut ppu, 0x0000), 0xAA);

    ppu.set_chr_bank(1);
    assert_eq!(read_vram(&mut ppu, 0x0000), 0xBB);
}

#[test]
fn reading_status_clears_vblank_and_write_toggle() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    ppu.start_vblank();
    ppu.cpu_write(0x2006, 0x20);
    assert!(ppu.w);

    let status = ppu.cpu_read(0x2002);
    assert_ne!(status & 0x80, 0);
    assert_eq!(ppu.status & 0x80, 0);
    assert!(!ppu.w);

    // The next PPUADDR write is the high byte again.
    ppu.cpu_write(0x2006, 0x21);
    assert_eq!(ppu.t & 0x3F00, 0x2100);
    assert!(ppu.w);
}

#[test]
fn pre_render_reloads_full_scroll_latch() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    ppu.t = 0x6F3F;
    ppu.v = 0x2000;
    ppu.reload_scroll();
    assert_eq!(ppu.v, 0x6F3F);
}

#[test]
fn vram_capture_records_nametable_and_palette_writes() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    ppu.frame = 12;
    ppu.line = 34;
    ppu.capture_vram = true;
    write_vram(&mut ppu, 0x2000, 0xAA);
    write_vram(&mut ppu, 0x3F05, 0x21);

    let writes = ppu.vram_writes();
    assert_eq!(writes.len(), 2);
    assert_eq!(
        writes[0],
        VramWrite {
            frame: 12,
            line: 34,
            addr: 0x2000,
            value: 0xAA
        }
    );
    assert_eq!(writes[1].addr, 0x3F05);
    assert_eq!(writes[1].value, 0x21);

    // Disabled captures stay empty.
    ppu.clear_vram_writes();
    ppu.capture_vram = false;
    write_vram(&mut ppu, 0x2001, 0x11);
    assert!(ppu.vram_writes().is_empty());
}

#[test]
fn snapshot_exposes_state_without_mutable_access() {
    let mut ppu = Ppu::new(&cart(Mirroring::Vertical));
    ppu.ctrl = 0xAB;
    ppu.mask = 0x1E;
    ppu.chr_bank = 2;
    ppu.fine_x = 5;
    write_vram(&mut ppu, 0x2000, 0xAA);
    ppu.palette[3] = 0x22;
    ppu.oam[0] = 1;
    ppu.oam[31] = 9;
    ppu.v = 0x2345;
    ppu.t = 0x1234;
    ppu.w = true;

    let snap = ppu.snapshot();
    assert_eq!(snap.ctrl, 0xAB);
    assert_eq!(snap.mask, 0x1E);
    assert_eq!(snap.chr_bank, 2);
    assert!(snap.w);
    assert_eq!(snap.v, 0x2345);
    assert_eq!(snap.t, 0x1234);
    assert_eq!(snap.fine_x, 5);
    assert_eq!(snap.palette[3], 0x22);
    assert_eq!(snap.oam[0], 1);
    assert_eq!(snap.oam[31], 9);
    assert_eq!(snap.nametable_checksum, fnv1a(&ppu.vram));
}

#[test]
fn coarse_x_wraps_to_next_nametable() {
    let v = 0x201F;
    assert_eq!(increment_coarse_x(v), 0x2400);
}

#[test]
fn vertical_increment_wraps_at_30() {
    // Coarse Y = 29, fine Y = 7 -> wrap to the next nametable.
    let v = 0x2000 | (29 << 5) | 0x7000;
    let next = increment_y(v);
    assert_eq!(next & 0x0800, 0x0800);
    assert_eq!((next >> 5) & 0x1F, 0);
}

/// A sprite-0 hit needs an opaque sprite-0 pixel over an opaque background
/// pixel. Also pins the OAM byte order: Y, tile, attributes, X.
#[test]
fn sprite_zero_hit_uses_oam_attribute_and_x_bytes() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high); // background tile, column 1
    let (low, high) = solid(3);
    set_tile(&mut chr, 0, 0x16C, low, high); // 8x16 sprite tile (bit 0 = 1)
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.ctrl = 0x20; // 8x16 sprites; PPUCTRL bit 3 deliberately clear
    ppu.mask = 0x1E;
    write_vram(&mut ppu, 0x2001, 1);
    // Y, tile, attributes, X. The background is only opaque at x = 8..15.
    ppu.oam[0..4].copy_from_slice(&[9, 0x6D, 0x00, 10]);
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer);
    assert_ne!(ppu.status & 0x40, 0, "sprite 0 should hit the background");
}

/// Pins the exact HUD scenario: the power-meter bar is sprite 0 at
/// `CE 6D 23 F8` (x = 248), 8x16, over an opaque background tile. The hit
/// must land on line 207 at the bar's right edge and be cleared with the
/// other status bits at pre-render.
#[test]
fn hud_power_meter_sprite_zero_hits_on_its_own_scanline() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high); // opaque HUD background tile
    let (low, high) = solid(3);
    set_tile(&mut chr, 0, 0x16C, low, high); // 8x16 sprite tile $6D
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.ctrl = 0xA8; // NMI, 8x16 sprites, sprite patterns at $1000
    ppu.mask = 0x1E; // background + sprites, no left clipping
    write_vram(&mut ppu, 0x233F, 1); // tile under x = 248..255 on row 25
    ppu.oam[0..4].copy_from_slice(&[0xCE, 0x6D, 0x23, 0xF8]);
    // Line 206: fine Y = 6, coarse Y = 25, horizontal scroll 0. The PPU
    // advances `v` itself at the end of the line, so line 207 follows.
    ppu.v = 0x2000 | (25 << 5) | 0x6000;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(206, &mut framebuffer);
    assert_eq!(ppu.status & 0x40, 0, "the line above the bar must not hit");
    assert_eq!(ppu.sprite_zero_hit, None);

    ppu.render_line(207, &mut framebuffer);
    assert_ne!(ppu.status & 0x40, 0, "sprite 0 should hit on line 207");
    assert_eq!(
        ppu.sprite_zero_hit,
        Some(SpriteZeroHit {
            line: 207,
            dot: 249
        })
    );

    // Pre-render clears the flag and the recorded hit, as the hardware
    // does at dot 1 of line 261.
    ppu.end_vblank();
    assert_eq!(ppu.status & 0x40, 0);
    assert_eq!(ppu.sprite_zero_hit, None);
}

#[test]
fn sprite_zero_hit_requires_opaque_background() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 0x10, low, high); // opaque sprite
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.ctrl = 0;
    ppu.mask = 0x1E;
    ppu.oam[0..4].copy_from_slice(&[9, 0x10, 0x00, 10]);
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(ppu.status & 0x40, 0);
}

#[test]
fn sprite_zero_hit_is_suppressed_by_left_clipping() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 0x10, low, high); // sprite tile
    set_tile(&mut chr, 0, 1, low, high); // background tile
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.ctrl = 0;
    write_vram(&mut ppu, 0x2000, 1);
    ppu.oam[0..4].copy_from_slice(&[9, 0x10, 0x00, 2]);
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    // Background left column shown, sprites clipped.
    ppu.mask = 0x1A;
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(ppu.status & 0x40, 0);

    // Sprites shown, background left column clipped.
    ppu.mask = 0x1C;
    ppu.v = 0;
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(ppu.status & 0x40, 0);

    ppu.mask = 0x1E;
    ppu.v = 0;
    ppu.render_line(10, &mut framebuffer);
    assert_ne!(ppu.status & 0x40, 0);
}

#[test]
fn lower_sprite_index_has_priority() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 0x00, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x10;
    ppu.palette[17] = 0x30;
    ppu.palette[21] = 0x31;
    ppu.oam[0..4].copy_from_slice(&[9, 0x00, 0x00, 10]);
    ppu.oam[4..8].copy_from_slice(&[9, 0x00, 0x01, 10]);

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 10, 10), color(0x30));
}

#[test]
fn only_eight_sprites_are_evaluated_per_line() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 0x00, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x10;
    ppu.palette[0] = 0x0F;
    ppu.palette[17] = 0x30;
    for sprite in 0..9 {
        let base = sprite * 4;
        let x = 16 + sprite * 8;
        ppu.oam[base..base + 4].copy_from_slice(&[9, 0x00, 0x00, x as u8]);
    }

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 16, 10), color(0x30));
    assert_eq!(pixel(&framebuffer, 80, 10), color(0x0F));
    assert_ne!(ppu.status & 0x20, 0, "ninth sprite sets overflow");
}

#[test]
fn vertical_flip_selects_bottom_row_first() {
    let mut chr = vec![0; 32 * 1024];
    let mut low = [0u8; 8];
    low[0] = 0xFF; // only the top row is opaque
    set_tile(&mut chr, 0, 0x20, low, [0; 8]);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x10;
    ppu.palette[0] = 0x0F;
    ppu.palette[17] = 0x30;
    ppu.oam[0..4].copy_from_slice(&[9, 0x20, 0x80, 10]);

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer); // source row 7
    assert_eq!(pixel(&framebuffer, 10, 10), color(0x0F));
    ppu.render_line(17, &mut framebuffer); // source row 0
    assert_eq!(pixel(&framebuffer, 10, 17), color(0x30));
}

#[test]
fn horizontal_flip_mirrors_the_tile() {
    let mut chr = vec![0; 32 * 1024];
    let mut low = [0u8; 8];
    low.fill(0x80); // only the left column is opaque
    set_tile(&mut chr, 0, 0x21, low, [0; 8]);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x10;
    ppu.palette[0] = 0x0F;
    ppu.palette[17] = 0x30;
    ppu.oam[0..4].copy_from_slice(&[9, 0x21, 0x40, 10]);

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 10, 10), color(0x0F));
    assert_eq!(pixel(&framebuffer, 17, 10), color(0x30));
}

#[test]
fn eight_by_sixteen_uses_second_tile_for_lower_half() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 0x03, low, high); // lower half, row 0
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.ctrl = 0x20;
    ppu.mask = 0x10;
    ppu.palette[0] = 0x0F;
    ppu.palette[17] = 0x30;
    ppu.oam[0..4].copy_from_slice(&[9, 0x02, 0x00, 10]);

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer); // upper tile (transparent)
    assert_eq!(pixel(&framebuffer, 10, 10), color(0x0F));
    ppu.render_line(18, &mut framebuffer); // lower tile row 0
    assert_eq!(pixel(&framebuffer, 10, 18), color(0x30));
}

#[test]
fn sprite_behind_background_is_hidden() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high);
    set_tile(&mut chr, 0, 0x10, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x1E;
    ppu.palette[1] = 0x30;
    ppu.palette[17] = 0x31;
    write_vram(&mut ppu, 0x2001, 1);
    ppu.v = 0;
    ppu.t = 0;
    ppu.oam[0..4].copy_from_slice(&[9, 0x10, 0x20, 10]);

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 10, 10), color(0x30));

    ppu.oam[2] = 0x00; // now in front
    ppu.v = 0;
    ppu.render_line(10, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 10, 10), color(0x31));
}

#[test]
fn mid_frame_scroll_write_applies_to_the_next_line() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high);
    let (low, high) = solid(2);
    set_tile(&mut chr, 0, 2, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x0A;
    ppu.palette[1] = 0x30;
    ppu.palette[2] = 0x31;
    write_vram(&mut ppu, 0x2000, 1); // column 0
    write_vram(&mut ppu, 0x2003, 2); // column 3
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(0, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 0), color(0x30));

    // Write coarse X = 3 during line 0; it must affect line 1.
    ppu.cpu_write(0x2005, 0x18);
    ppu.render_line(1, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 1), color(0x31));
}

#[test]
fn mid_frame_ppumask_can_disable_rendering() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x0A;
    ppu.palette[0] = 0x0F;
    ppu.palette[1] = 0x30;
    write_vram(&mut ppu, 0x2000, 1);
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(0, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 0), color(0x30));

    ppu.cpu_write(0x2001, 0x00);
    ppu.render_line(1, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 1), color(0x0F));
}

#[test]
fn mid_frame_ppuctrl_selects_background_pattern_table() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high);
    let (low, high) = solid(2);
    set_tile(&mut chr, 0, 0x101, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x0A;
    ppu.palette[1] = 0x30;
    ppu.palette[2] = 0x31;
    write_vram(&mut ppu, 0x2000, 1);
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(0, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 0), color(0x30));

    ppu.cpu_write(0x2000, 0x10);
    ppu.render_line(1, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 1), color(0x31));
}

#[test]
fn chr_bank_latches_per_rendered_scanline() {
    let mut chr = vec![0; 32 * 1024];
    let (low, high) = solid(1);
    set_tile(&mut chr, 0, 1, low, high);
    let (low, high) = solid(2);
    set_tile(&mut chr, 1, 1, low, high);
    let mut ppu = Ppu::new(&cart_with(Mirroring::Vertical, chr));
    ppu.mask = 0x0A;
    ppu.palette[1] = 0x30;
    ppu.palette[2] = 0x31;
    write_vram(&mut ppu, 0x2000, 1);
    ppu.v = 0;
    ppu.t = 0;

    let mut framebuffer = vec![0; FRAMEBUFFER_LEN];
    ppu.render_line(0, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 0), color(0x30));

    // A mapper write during HBlank of line 0 is visible on line 1.
    ppu.set_chr_bank(1);
    ppu.v = 0;
    ppu.render_line(1, &mut framebuffer);
    assert_eq!(pixel(&framebuffer, 0, 1), color(0x31));
}
