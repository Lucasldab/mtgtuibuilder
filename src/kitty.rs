//! Kitty graphics via unicode placeholders, emitted per cell.
//!
//! ratatui-image draws a whole row of placeholders into a *single* ratatui
//! cell and marks the rest of the row skipped, which its source acknowledges
//! as a workaround. tmux cannot represent that -- its grid stores one glyph
//! plus a few combining marks per cell -- so the placeholder grid arrives
//! malformed and kitty places no image, leaving an empty pane with no error
//! (the transmission asks for `q=2`, which silences every reply).
//!
//! `kitten icat` works in the same tmux by writing each cell separately with
//! its own row, column and image-id diacritics. This module does the same, so
//! ratatui writes one placeholder per cell and tmux forwards it intact.
//!
//! The image is transmitted as PNG straight to stdout, outside ratatui's
//! buffer, chunked and (under tmux) wrapped per chunk in a passthrough.

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::DynamicImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use std::io::Write;

/// The character kitty reserves for image placeholders.
const PLACEHOLDER: char = '\u{10EEEE}';

/// Kitty's documented chunk size for transmission payloads.
const CHUNK: usize = 4096;

/// Kitty's row/column diacritics, from its rowcolumn-diacritics.txt.
/// Table copied from ratatui-image (MIT).
static DIACRITICS: [char; 297] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

fn diacritic(n: u16) -> char {
    *DIACRITICS.get(n as usize).unwrap_or(&DIACRITICS[0])
}

/// A transmitted image: its kitty id and the cell box it was sized for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub id: u32,
    pub cols: u16,
    pub rows: u16,
}

/// Fits `img` into `area` at the terminal's font size, returning the pixel
/// size to transmit and the cell box it occupies.
pub fn fit(img: &DynamicImage, area: Rect, font: (u16, u16)) -> (u32, u32, u16, u16) {
    let (fw, fh) = (font.0.max(1) as f32, font.1.max(1) as f32);
    let avail_w = area.width as f32 * fw;
    let avail_h = area.height as f32 * fh;
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let scale = (avail_w / iw).min(avail_h / ih).min(1.0);
    let cols = ((iw * scale) / fw).floor().max(1.0) as u16;
    let rows = ((ih * scale) / fh).floor().max(1.0) as u16;
    // Transmit exactly the cell box in pixels so kitty scales 1:1.
    (cols as u32 * font.0 as u32, rows as u32 * font.1 as u32, cols, rows)
}

/// Encodes and sends the image. Writes straight to stdout because the
/// transmission is not cell content -- ratatui must never see it.
pub fn transmit(
    img: &DynamicImage,
    place: Placement,
    px: (u32, u32),
    is_tmux: bool,
) -> Result<()> {
    let resized = img.resize_exact(px.0, px.1, image::imageops::FilterType::Lanczos3);
    let mut png = Vec::new();
    resized
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let payload = STANDARD.encode(&png);

    let mut out = String::with_capacity(payload.len() + 4096);
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(CHUNK)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let last = i + 1 == chunks.len();
        // Only the first chunk carries the full control data; f=100 is PNG,
        // U=1 a virtual placement, q=2 silences replies.
        let control = if i == 0 {
            format!(
                "a=T,U=1,i={},f=100,q=2,c={},r={},m={}",
                place.id,
                place.cols,
                place.rows,
                u8::from(!last)
            )
        } else {
            format!("m={}", u8::from(!last))
        };
        let seq = format!("\x1b_G{control};{chunk}\x1b\\");
        if is_tmux {
            // One passthrough per chunk, rather than one wrapping everything.
            out.push_str("\x1bPtmux;");
            out.push_str(&seq.replace('\x1b', "\x1b\x1b"));
            out.push_str("\x1b\\");
        } else {
            out.push_str(&seq);
        }
    }

    let mut stdout = std::io::stdout();
    stdout.write_all(out.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

/// Writes one placeholder per cell. Each carries its own row, column and
/// image-id diacritics, and the id's low three bytes ride in the foreground
/// colour -- exactly what kitty expects, and what survives tmux.
pub fn draw(area: Rect, buf: &mut Buffer, place: Placement) {
    let [id_extra, r, g, b] = place.id.to_be_bytes();
    let style = Style::default().fg(Color::Rgb(r, g, b));

    for row in 0..place.rows.min(area.height) {
        for col in 0..place.cols.min(area.width) {
            let Some(cell) = buf.cell_mut((area.left() + col, area.top() + row)) else {
                continue;
            };
            let mut symbol = String::with_capacity(16);
            symbol.push(PLACEHOLDER);
            symbol.push(diacritic(row));
            symbol.push(diacritic(col));
            symbol.push(diacritic(u16::from(id_extra)));
            cell.set_symbol(&symbol);
            cell.set_style(style);
        }
    }
}

/// Frees a transmitted image so kitty does not accumulate them.
pub fn delete(id: u32, is_tmux: bool) {
    let seq = format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
    let wrapped = if is_tmux {
        format!("\x1bPtmux;{}\x1b\\", seq.replace('\x1b', "\x1b\x1b"))
    } else {
        seq
    };
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(wrapped.as_bytes());
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diacritics_cover_the_table() {
        assert_eq!(DIACRITICS.len(), 297);
        assert_eq!(diacritic(0), DIACRITICS[0]);
        assert_eq!(diacritic(296), DIACRITICS[296]);
        // Out of range must not panic; row 0 is the safe fallback.
        assert_eq!(diacritic(9999), DIACRITICS[0]);
    }

    #[test]
    fn fit_preserves_aspect_within_the_area() {
        let img = DynamicImage::new_rgb8(488, 680);
        let area = Rect::new(0, 0, 36, 23);
        let (w, h, cols, rows) = fit(&img, area, (12, 27));
        assert!(cols <= 36 && rows <= 23, "{cols}x{rows} overflows the area");
        assert_eq!(w, cols as u32 * 12);
        assert_eq!(h, rows as u32 * 27);
        // Portrait card stays portrait.
        assert!(h > w);
    }

    #[test]
    fn fit_never_upscales() {
        let img = DynamicImage::new_rgb8(40, 40);
        let (w, h, _, _) = fit(&img, Rect::new(0, 0, 200, 200), (12, 27));
        assert!(w <= 48 && h <= 54, "upscaled to {w}x{h}");
    }

    #[test]
    fn every_cell_gets_its_own_placeholder() {
        // The whole point: one glyph per cell, not a row crammed into one.
        let area = Rect::new(0, 0, 10, 4);
        let mut buf = Buffer::empty(area);
        let place = Placement { id: 0x01020304, cols: 6, rows: 3 };
        draw(area, &mut buf, place);

        for row in 0..3u16 {
            for col in 0..6u16 {
                let sym = buf[(col, row)].symbol();
                let chars: Vec<char> = sym.chars().collect();
                assert_eq!(chars[0], PLACEHOLDER, "cell {col},{row} is not a placeholder");
                assert_eq!(chars[1], diacritic(row), "wrong row diacritic at {col},{row}");
                assert_eq!(chars[2], diacritic(col), "wrong column diacritic at {col},{row}");
                assert_eq!(chars.len(), 4, "expected exactly three diacritics");
            }
        }
        // Cells outside the placement are untouched.
        assert_ne!(buf[(7, 0)].symbol(), buf[(0, 0)].symbol());
    }

    #[test]
    fn placeholder_colour_carries_the_image_id() {
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        draw(area, &mut buf, Placement { id: 0x00AABBCC, cols: 2, rows: 1 });
        assert_eq!(buf[(0, 0)].style().fg, Some(Color::Rgb(0xAA, 0xBB, 0xCC)));
    }

    #[test]
    fn draw_is_clipped_to_the_area() {
        let area = Rect::new(0, 0, 3, 2);
        let mut buf = Buffer::empty(area);
        // Placement larger than the area must not panic or write out of bounds.
        draw(area, &mut buf, Placement { id: 1, cols: 99, rows: 99 });
    }
}
