use super::ScreenData;

/// Tile solidity from the reconstructed map.
///
/// The original's packed terrain map (`$0500`/`$0600`, read by `$C39D` with
/// `$C409` masks) stays zero in the oracle, so there is no cartridge tile
/// table to copy. Instead the two structural fill tiles are measured from the
/// map, the starfield set is measured where no fill occurs, and a tile is
/// solid when it belongs to a region connected to a fill tile. Isolated
/// twinkles and comet streaks are therefore never solid.
pub fn classify_solid(screen: &ScreenData) -> [bool; 256] {
    let width = screen.width_tiles;
    let height = screen.height_tiles;
    let tiles = &screen.tiles;

    let most_common = |rows: std::ops::Range<usize>| -> u8 {
        let mut counts = [0usize; 256];
        for row in rows {
            for column in 0..width {
                counts[usize::from(tiles[row * width + column])] += 1;
            }
        }
        counts[0] = 0; // empty is never a fill tile
        counts
            .iter()
            .enumerate()
            .max_by_key(|(tile, count)| (**count, std::cmp::Reverse(*tile)))
            .map(|(tile, _)| tile as u8)
            .unwrap_or(0)
    };
    let ceiling_fill = most_common(0..3.min(height));
    let floor_fill = most_common(height.saturating_sub(3)..height);

    // Values seen in columns that contain neither fill tile are the open-space
    // (starfield) decoration.
    let mut decorative = [false; 256];
    for column in 0..width {
        let mut open = true;
        for row in 0..height {
            let tile = tiles[row * width + column];
            if tile == ceiling_fill || tile == floor_fill {
                open = false;
                break;
            }
        }
        if open {
            for row in 0..height {
                decorative[usize::from(tiles[row * width + column])] = true;
            }
        }
    }
    decorative[0] = false;

    let mut solid = [false; 256];
    let mut seen = vec![false; width * height];
    for start in 0..width * height {
        if seen[start] || tiles[start] == 0 {
            continue;
        }
        let mut stack = vec![start];
        let mut region = Vec::new();
        let mut touches_fill = false;
        seen[start] = true;
        while let Some(index) = stack.pop() {
            region.push(index);
            let tile = tiles[index];
            if tile == ceiling_fill || tile == floor_fill {
                touches_fill = true;
            }
            let x = index % width;
            let y = index / width;
            let neighbours = [
                (x > 0).then_some(index - 1),
                (x + 1 < width).then_some(index + 1),
                (y > 0).then_some(index - width),
                (y + 1 < height).then_some(index + width),
            ];
            for neighbour in neighbours.into_iter().flatten() {
                if !seen[neighbour] && tiles[neighbour] != 0 {
                    seen[neighbour] = true;
                    stack.push(neighbour);
                }
            }
        }
        if touches_fill {
            for index in region {
                let tile = tiles[index];
                if !decorative[usize::from(tile)] {
                    solid[usize::from(tile)] = true;
                }
            }
        }
    }
    solid[0] = false;
    solid
}

pub fn hex(palette: &[u8; 32]) -> String {
    palette
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
