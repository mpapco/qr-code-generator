//! QR payment code generation (SS 5.3): ISO/IEC 18004 with error correction
//! level M, rendered to the terminal, to PNG and to SVG.

use std::path::Path;

use anyhow::{Context, Result};
use qrcode::{Color, EcLevel, QrCode};

/// Modules of white margin around the symbol, as required by ISO/IEC 18004.
pub const QUIET_ZONE: usize = 4;

/// A rendered QR symbol including its quiet zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    /// Side length in modules, quiet zone included.
    pub size: usize,
    dark: Vec<bool>,
}

impl Matrix {
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.size + x]
    }
}

/// Encode `data` at error correction level M, as the standard prescribes.
pub fn encode(data: &str) -> Result<Matrix> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M)
        .context("the payment link does not fit into a QR code")?;

    let width = code.width();
    let size = width + 2 * QUIET_ZONE;
    let colors = code.to_colors();
    let mut dark = vec![false; size * size];

    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == Color::Dark {
                dark[(y + QUIET_ZONE) * size + (x + QUIET_ZONE)] = true;
            }
        }
    }

    Ok(Matrix { size, dark })
}

/// How a symbol is drawn in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStyle {
    /// One character per two module rows, using the upper half block.
    HalfBlocks,
    /// Two characters per module: taller, but correct in terminals that render
    /// half blocks with gaps.
    FullBlocks,
}

impl TerminalStyle {
    /// Terminal cells the symbol needs: (columns, rows).
    pub fn cell_size(self, matrix: &Matrix) -> (usize, usize) {
        match self {
            TerminalStyle::HalfBlocks => (matrix.size, matrix.size.div_ceil(2)),
            TerminalStyle::FullBlocks => (matrix.size * 2, matrix.size),
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            TerminalStyle::HalfBlocks => TerminalStyle::FullBlocks,
            TerminalStyle::FullBlocks => TerminalStyle::HalfBlocks,
        }
    }
}

/// The colour of a module pair in one terminal cell: (top, bottom), true = dark.
/// Rows are produced two module rows at a time for [`TerminalStyle::HalfBlocks`].
pub fn half_block_rows(matrix: &Matrix) -> Vec<Vec<(bool, bool)>> {
    (0..matrix.size)
        .step_by(2)
        .map(|y| {
            (0..matrix.size)
                .map(|x| {
                    let top = matrix.is_dark(x, y);
                    // Pad an odd-sized symbol with a light row.
                    let bottom = y + 1 < matrix.size && matrix.is_dark(x, y + 1);
                    (top, bottom)
                })
                .collect()
        })
        .collect()
}

/// The symbol as a self-contained ANSI string, dark modules on a white ground.
///
/// Explicit background colours are used rather than glyph colours so the symbol
/// keeps the right polarity in both light and dark terminals.
pub fn to_ansi(matrix: &Matrix, style: TerminalStyle) -> String {
    const WHITE_BG: &str = "\x1b[47m";
    const BLACK_BG: &str = "\x1b[40m";
    const RESET: &str = "\x1b[0m";

    let mut out = String::new();
    match style {
        TerminalStyle::FullBlocks => {
            for y in 0..matrix.size {
                for x in 0..matrix.size {
                    out.push_str(if matrix.is_dark(x, y) {
                        BLACK_BG
                    } else {
                        WHITE_BG
                    });
                    out.push_str("  ");
                }
                out.push_str(RESET);
                out.push('\n');
            }
        }
        TerminalStyle::HalfBlocks => {
            for row in half_block_rows(matrix) {
                for (top, bottom) in row {
                    // Upper half block: foreground paints the top module,
                    // background the bottom one.
                    out.push_str(if top { "\x1b[30m" } else { "\x1b[37m" });
                    out.push_str(if bottom { BLACK_BG } else { WHITE_BG });
                    out.push('\u{2580}');
                }
                out.push_str(RESET);
                out.push('\n');
            }
        }
    }
    out
}

/// Write the symbol as a PNG, `scale` pixels per module.
pub fn write_png(matrix: &Matrix, path: &Path, scale: u32) -> Result<()> {
    let scale = scale.max(1);
    let side = matrix.size as u32 * scale;
    let mut image = image::GrayImage::from_pixel(side, side, image::Luma([255u8]));

    for y in 0..matrix.size {
        for x in 0..matrix.size {
            if !matrix.is_dark(x, y) {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    image.put_pixel(
                        x as u32 * scale + dx,
                        y as u32 * scale + dy,
                        image::Luma([0u8]),
                    );
                }
            }
        }
    }

    image
        .save(path)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Render the symbol as SVG. Horizontal runs of dark modules are merged into a
/// single rectangle to keep the file small.
pub fn to_svg(matrix: &Matrix, scale: u32) -> String {
    let scale = scale.max(1);
    let side = matrix.size as u32 * scale;

    let mut rects = String::new();
    for y in 0..matrix.size {
        let mut x = 0;
        while x < matrix.size {
            if !matrix.is_dark(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < matrix.size && matrix.is_dark(x, y) {
                x += 1;
            }
            rects.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                start as u32 * scale,
                y as u32 * scale,
                (x - start) as u32 * scale,
                scale
            ));
        }
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{side}\" height=\"{side}\" \
         viewBox=\"0 0 {side} {side}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{side}\" height=\"{side}\" fill=\"#ffffff\"/>\
         <g fill=\"#000000\">{rects}</g></svg>\n"
    )
}

/// Write the symbol as an SVG file.
pub fn write_svg(matrix: &Matrix, path: &Path, scale: u32) -> Result<()> {
    std::fs::write(path, to_svg(matrix, scale))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "https://payme.sk/2/q/PME?IBAN=SK6807200002891987426353&CN=Hope+charity";

    #[test]
    fn the_quiet_zone_surrounds_the_symbol() {
        let matrix = encode(LINK).unwrap();
        // A version-N symbol is 4N+17 modules wide, plus two quiet zones.
        assert_eq!((matrix.size - 2 * QUIET_ZONE) % 4, 1);

        for i in 0..matrix.size {
            for q in 0..QUIET_ZONE {
                assert!(!matrix.is_dark(i, q), "top quiet zone");
                assert!(!matrix.is_dark(q, i), "left quiet zone");
                assert!(!matrix.is_dark(i, matrix.size - 1 - q), "bottom quiet zone");
                assert!(!matrix.is_dark(matrix.size - 1 - q, i), "right quiet zone");
            }
        }
    }

    #[test]
    fn finder_pattern_is_where_it_should_be() {
        let matrix = encode(LINK).unwrap();
        // Top-left finder: a 7x7 dark ring starting right after the quiet zone.
        for i in 0..7 {
            assert!(matrix.is_dark(QUIET_ZONE + i, QUIET_ZONE));
            assert!(matrix.is_dark(QUIET_ZONE, QUIET_ZONE + i));
        }
        assert!(!matrix.is_dark(QUIET_ZONE + 1, QUIET_ZONE + 1));
    }

    #[test]
    fn half_block_rows_cover_every_module() {
        let matrix = encode(LINK).unwrap();
        let rows = half_block_rows(&matrix);
        assert_eq!(rows.len(), matrix.size.div_ceil(2));
        assert!(rows.iter().all(|r| r.len() == matrix.size));
    }

    #[test]
    fn cell_size_matches_the_rendered_output() {
        let matrix = encode(LINK).unwrap();
        for style in [TerminalStyle::HalfBlocks, TerminalStyle::FullBlocks] {
            let (_, rows) = style.cell_size(&matrix);
            assert_eq!(to_ansi(&matrix, style).lines().count(), rows);
        }
    }

    #[test]
    fn svg_declares_the_scaled_side() {
        let matrix = encode(LINK).unwrap();
        let svg = to_svg(&matrix, 4);
        let side = matrix.size * 4;
        assert!(svg.contains(&format!("viewBox=\"0 0 {side} {side}\"")));
        assert!(svg.contains("<rect"));
    }
}
