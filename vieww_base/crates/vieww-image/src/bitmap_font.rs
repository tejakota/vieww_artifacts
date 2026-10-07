//! Bitmap fonts — PixiJS' `BitmapText`, Phaser's `BitmapText`, every game
//! engine's atlas text: glyphs pre-rendered into a texture, laid out by
//! metrics, drawn as image quads. Fast (no shaping, no rasterising), exact
//! (the artist's pixels), and the format tools export.
//!
//! [`BitmapFont::parse_fnt`] reads AngelCode BMFont's text format (`info`,
//! `common`, `page`, `char`, `kerning` lines — what Hiero, BMFont,
//! Littera and msdf-bmfont emit). [`BitmapFont::layout`] places glyphs
//! with advances, offsets, kerning, line height and alignment;
//! [`BitmapFont::draw`] records each glyph into a
//! [`Sketchbook`] as its atlas sub-rectangle.

use std::collections::HashMap;

use vieww_foundation::{Image, Offset, Path, Rect, Sketchbook, Transform};

/// One glyph's atlas rectangle and metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub xoffset: f32,
    pub yoffset: f32,
    pub xadvance: f32,
    pub page: usize,
}

/// A parsed bitmap font.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitmapFont {
    pub face: String,
    pub size: f32,
    pub line_height: f32,
    pub base: f32,
    pub pages: Vec<String>,
    pub glyphs: HashMap<char, Glyph>,
    pub kerning: HashMap<(char, char), f32>,
}

/// Horizontal alignment of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// A placed glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub ch: char,
    pub glyph: Glyph,
    /// Top-left of the glyph quad in layout space.
    pub at: Offset,
}

fn attrs(line: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut rest = line;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].split_whitespace().last().unwrap_or("").to_owned();
        let after = &rest[eq + 1..];
        let (val, next) = if let Some(stripped) = after.strip_prefix('"') {
            let end = stripped.find('"').unwrap_or(stripped.len());
            (stripped[..end].to_owned(), &stripped[(end + 1).min(stripped.len())..])
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (after[..end].to_owned(), &after[end..])
        };
        m.insert(key, val);
        rest = next;
    }
    m
}

impl BitmapFont {
    /// Parse the BMFont text format.
    ///
    /// # Errors
    /// No `common` line or no glyphs.
    pub fn parse_fnt(text: &str) -> Result<Self, String> {
        let mut f = Self::default();
        let num = |m: &HashMap<String, String>, k: &str| m.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let mut saw_common = false;
        for line in text.lines() {
            let tag = line.split_whitespace().next().unwrap_or("");
            let a = attrs(line);
            match tag {
                "info" => {
                    f.face = a.get("face").cloned().unwrap_or_default();
                    f.size = num(&a, "size").abs();
                }
                "common" => {
                    f.line_height = num(&a, "lineHeight");
                    f.base = num(&a, "base");
                    saw_common = true;
                }
                "page" => f.pages.push(a.get("file").cloned().unwrap_or_default()),
                "char" => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let id = num(&a, "id") as u32;
                    if let Some(c) = char::from_u32(id) {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        f.glyphs.insert(c, Glyph {
                            x: num(&a, "x"),
                            y: num(&a, "y"),
                            width: num(&a, "width"),
                            height: num(&a, "height"),
                            xoffset: num(&a, "xoffset"),
                            yoffset: num(&a, "yoffset"),
                            xadvance: num(&a, "xadvance"),
                            page: num(&a, "page") as usize,
                        });
                    }
                }
                "kerning" => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let (p, q) = (num(&a, "first") as u32, num(&a, "second") as u32);
                    if let (Some(p), Some(q)) = (char::from_u32(p), char::from_u32(q)) {
                        f.kerning.insert((p, q), num(&a, "amount"));
                    }
                }
                _ => {}
            }
        }
        if !saw_common {
            return Err("BMFont: no common line".into());
        }
        if f.glyphs.is_empty() {
            return Err("BMFont: no glyphs".into());
        }
        Ok(f)
    }

    /// Lay out `text` (newlines break lines; unknown characters use `?`
    /// or are skipped), wrapping at `max_width` when given.
    #[must_use]
    pub fn layout(&self, text: &str, align: Align, max_width: Option<f32>) -> (Vec<Placed>, f32, f32) {
        let mut lines: Vec<Vec<Placed>> = vec![Vec::new()];
        let mut widths = vec![0.0f32];
        let mut x = 0.0f32;
        let mut prev: Option<char> = None;
        for ch in text.chars() {
            if ch == '\n' {
                lines.push(Vec::new());
                widths.push(0.0);
                x = 0.0;
                prev = None;
                continue;
            }
            let Some(g) = self.glyphs.get(&ch).or_else(|| self.glyphs.get(&'?')).copied() else {
                continue;
            };
            let kern = prev.and_then(|p| self.kerning.get(&(p, ch))).copied().unwrap_or(0.0);
            if let Some(mw) = max_width {
                if x + kern + g.xadvance > mw && x > 0.0 {
                    // Wrap at the last space on this line, if there is one.
                    let line = lines.last_mut().expect("a line");
                    if let Some(sp) = line.iter().rposition(|p| p.ch == ' ') {
                        let tail: Vec<Placed> = line.drain(sp + 1..).collect();
                        line.pop();
                        let shift = tail.first().map_or(0.0, |p| p.at.dx - p.glyph.xoffset);
                        let moved: Vec<Placed> = tail
                            .into_iter()
                            .map(|mut p| {
                                p.at.dx -= shift;
                                p
                            })
                            .collect();
                        *widths.last_mut().expect("w") = line.last().map_or(0.0, |p| p.at.dx - p.glyph.xoffset + p.glyph.xadvance);
                        x = moved.last().map_or(0.0, |p| p.at.dx - p.glyph.xoffset + p.glyph.xadvance);
                        lines.push(moved);
                        widths.push(x);
                    } else {
                        lines.push(Vec::new());
                        widths.push(0.0);
                        x = 0.0;
                    }
                }
            }
            x += kern;
            lines.last_mut().expect("a line").push(Placed {
                ch,
                glyph: g,
                at: Offset::new(x + g.xoffset, g.yoffset),
            });
            x += g.xadvance;
            *widths.last_mut().expect("w") = x;
            prev = Some(ch);
        }
        let total_w = widths.iter().copied().fold(0.0, f32::max);
        let mut out = Vec::new();
        for (i, (line, w)) in lines.into_iter().zip(&widths).enumerate() {
            let dx = match align {
                Align::Left => 0.0,
                Align::Center => (total_w - w) * 0.5,
                Align::Right => total_w - w,
            };
            #[allow(clippy::cast_precision_loss)]
            let dy = i as f32 * self.line_height;
            out.extend(line.into_iter().map(|mut p| {
                p.at = Offset::new(p.at.dx + dx, p.at.dy + dy);
                p
            }));
        }
        #[allow(clippy::cast_precision_loss)]
        let h = widths.len() as f32 * self.line_height;
        (out, total_w, h)
    }

    /// Record `text` at `origin` (top-left), scaled by `scale`, using the
    /// atlas `pages`.
    pub fn draw(&self, book: &mut Sketchbook, pages: &[Image], text: &str, origin: Offset, scale: f32, align: Align) {
        let (placed, _, _) = self.layout(text, align, None);
        for p in placed {
            let g = p.glyph;
            if g.width <= 0.0 || g.height <= 0.0 {
                continue;
            }
            let Some(page) = pages.get(g.page) else { continue };
            let dst = Rect::new(
                origin.dx + p.at.dx * scale,
                origin.dy + p.at.dy * scale,
                origin.dx + (p.at.dx + g.width) * scale,
                origin.dy + (p.at.dy + g.height) * scale,
            );
            // Show the atlas sub-rectangle through a clip at the glyph's place.
            let (pw, ph) = (page.width() as f32, page.height() as f32);
            let img = page.clone();
            book.layer(1.0, 0.0, Some(Path::rect(dst)), |b| {
                b.transformed(
                    Transform::new(scale, 0.0, 0.0, scale, dst.left - g.x * scale, dst.top - g.y * scale),
                    |k| {
                        k.image(Rect::new(0.0, 0.0, pw, ph), img);
                    },
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FNT: &str = r#"info face="Pixel" size=16 bold=0
common lineHeight=20 base=16 scaleW=64 scaleH=64 pages=1
page id=0 file="pixel_0.png"
chars count=4
char id=65 x=0 y=0 width=10 height=12 xoffset=1 yoffset=4 xadvance=11 page=0
char id=86 x=10 y=0 width=10 height=12 xoffset=0 yoffset=4 xadvance=10 page=0
char id=32 x=0 y=0 width=0 height=0 xoffset=0 yoffset=0 xadvance=5 page=0
char id=63 x=20 y=0 width=8 height=12 xoffset=0 yoffset=4 xadvance=9 page=0
kernings count=1
kerning first=65 second=86 amount=-2
"#;

    #[test]
    fn parses_bmfont_text() {
        let f = BitmapFont::parse_fnt(FNT).unwrap();
        assert_eq!(f.face, "Pixel");
        assert_eq!((f.line_height, f.base), (20.0, 16.0));
        assert_eq!(f.pages, vec!["pixel_0.png"]);
        assert_eq!(f.glyphs.len(), 4);
        assert_eq!(f.kerning[&('A', 'V')], -2.0);
        assert!(BitmapFont::parse_fnt("info face=x").is_err());
    }

    #[test]
    fn layout_applies_advance_and_kerning() {
        let f = BitmapFont::parse_fnt(FNT).unwrap();
        let (p, w, h) = f.layout("AV", Align::Left, None);
        assert_eq!(p[0].at, Offset::new(1.0, 4.0));
        assert_eq!(p[1].at.dx, 11.0 - 2.0 + 0.0, "kerned");
        assert_eq!(w, 19.0);
        assert_eq!(h, 20.0);
        let (q, _, _) = f.layout("AZ", Align::Left, None);
        assert_eq!(q[1].ch, 'Z');
        assert_eq!(q[1].glyph.x, 20.0, "unknown glyphs fall back to '?'");
    }

    #[test]
    fn lines_align_and_wrap() {
        let f = BitmapFont::parse_fnt(FNT).unwrap();
        let (p, w, _) = f.layout("AVA\nA", Align::Right, None);
        let last = p.last().unwrap();
        assert!((last.at.dx - (w - 11.0 + 1.0)).abs() < 1e-4, "right-aligned");
        assert_eq!(last.at.dy, 24.0, "second line");
        let (wrapped, ww, hh) = f.layout("AA AA AA", Align::Left, Some(30.0));
        assert!(ww <= 30.0);
        assert_eq!(hh, 60.0, "three lines");
        assert_eq!(wrapped.iter().filter(|p| p.ch == 'A').count(), 6);
    }

    #[test]
    fn draw_records_one_clipped_quad_per_visible_glyph() {
        let f = BitmapFont::parse_fnt(FNT).unwrap();
        let page = Image::from_rgba8(vec![255; 64 * 64 * 4], 64, 64);
        let mut book = Sketchbook::new();
        f.draw(&mut book, &[page], "A V", Offset::new(10.0, 10.0), 2.0, Align::Left);
        assert_eq!(book.len(), 2, "the space has no quad");
    }
}
