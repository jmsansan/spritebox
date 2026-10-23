//! TrueType writing for pixel fonts.
//!
//! The file has the ten tables that TrueType requires: `OS/2`, `cmap`, `glyf`, `head`,
//! `hhea`, `hmtx`, `loca`, `maxp`, `name` and `post`. Outlines are straight lines on
//! a grid of `units_per_pixel` font units, and the em is the glyph height, so the
//! font is crisp at integer multiples of its pixel size. Nothing depends on the time
//! or the platform: the same font gives the same bytes.

use crate::font::{Font, Glyph, TtfOptions, outline};
use crate::{Error, ErrorCode, Result};

struct Outline {
    contours: Vec<Vec<(i32, i32)>>,
    advance: i32,
}

impl Outline {
    fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut points = self.contours.iter().flatten();
        let &(x, y) = points.next()?;
        Some(points.fold((x, y, x, y), |(x0, y0, x1, y1), &(x, y)| {
            (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
        }))
    }
}

pub(crate) fn write(font: &Font, options: &TtfOptions) -> Result<Vec<u8>> {
    font.validate()?;
    let u = font.units_per_pixel() as i32;
    if font.height as i64 * u as i64 > 16384
        || font
            .glyphs
            .iter()
            .any(|g| g.width.max(g.advance) as i64 * u as i64 > 32000)
    {
        return Err(Error::new(
            ErrorCode::EncodeError,
            "The font is too large for TrueType coordinates",
        ));
    }
    let ascent = font.baseline as i32 * u;
    let descent = (font.height - font.baseline) as i32 * u;
    // Glyph 0 is .notdef: a box with a hole, as wide as half the height.
    let box_width = (font.height / 2).max(1) as i32;
    let mut notdef = vec![vec![
        (0, ascent),
        (box_width * u, ascent),
        (box_width * u, -descent),
        (0, -descent),
    ]];
    if box_width >= 3 && font.height >= 3 {
        notdef.push(vec![
            (u, ascent - u),
            (u, -descent + u),
            ((box_width - 1) * u, -descent + u),
            ((box_width - 1) * u, ascent - u),
        ]);
    }
    let mut outlines = vec![Outline {
        contours: notdef,
        advance: (box_width + 1) * u,
    }];
    for glyph in &font.glyphs {
        outlines.push(Outline {
            contours: outline(glyph, font.height, font.baseline)
                .into_iter()
                .map(|c| c.into_iter().map(|(x, y)| (x * u, y * u)).collect())
                .collect(),
            advance: glyph.advance as i32 * u,
        });
    }
    let family = options.family.clone().unwrap_or_else(|| font.name.clone());
    let mut tables = vec![
        (*b"OS/2", os2(font, &outlines, u)),
        (*b"cmap", cmap(&font.glyphs)),
        (*b"glyf", Vec::new()),
        (*b"head", Vec::new()),
        (*b"hhea", hhea(font, &outlines, u)),
        (*b"hmtx", hmtx(&outlines)),
        (*b"loca", Vec::new()),
        (*b"maxp", maxp(&outlines)),
        (*b"name", name(&family, options.version)),
        (*b"post", post(&outlines, u)),
    ];
    if !font.kerning.is_empty() {
        tables.push((*b"kern", kern(font, u)?));
        tables.sort_by_key(|(tag, _)| *tag);
    }
    let (glyf, loca) = glyf_and_loca(&outlines);
    let head = head(font, &outlines, u, options.version);
    let tables: Vec<([u8; 4], Vec<u8>)> = tables
        .into_iter()
        .map(|(tag, data)| match &tag {
            b"glyf" => (tag, glyf.clone()),
            b"loca" => (tag, loca.clone()),
            b"head" => (tag, head.clone()),
            _ => (tag, data),
        })
        .collect();
    Ok(assemble(tables))
}

fn assemble(tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let n = tables.len() as u16;
    let power = 1u16 << (15 - n.leading_zeros() as u16);
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&(power * 16).to_be_bytes());
    out.extend_from_slice(&(power.trailing_zeros() as u16).to_be_bytes());
    out.extend_from_slice(&(n * 16 - power * 16).to_be_bytes());
    let mut offset = 12 + 16 * tables.len();
    let mut head_offset = 0;
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        if tag == b"head" {
            head_offset = offset;
        }
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    let adjustment = 0xb1b0_afbau32.wrapping_sub(checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    out
}

fn kern(font: &Font, units: i32) -> Result<Vec<u8>> {
    let mut pairs = Vec::new();
    for pair in &font.kerning {
        let left = font
            .glyphs
            .iter()
            .position(|g| g.ch == pair.left)
            .ok_or_else(|| Error::new(ErrorCode::EncodeError, "Unknown left kerning glyph"))?
            + 1;
        let right = font
            .glyphs
            .iter()
            .position(|g| g.ch == pair.right)
            .ok_or_else(|| Error::new(ErrorCode::EncodeError, "Unknown right kerning glyph"))?
            + 1;
        let value = pair.adjustment as i64 * units as i64;
        if !(i16::MIN as i64..=i16::MAX as i64).contains(&value) {
            return Err(Error::new(
                ErrorCode::EncodeError,
                "Kerning exceeds TrueType coordinates",
            ));
        }
        pairs.push((left as u16, right as u16, value as i32));
    }
    pairs.sort_by_key(|&(l, r, _)| (l, r));
    if pairs.len() > (u16::MAX as usize - 14) / 6 {
        return Err(Error::new(
            ErrorCode::EncodeError,
            "Too many kerning pairs for a format-0 kern table",
        ));
    }
    let n = pairs.len() as u16;
    let power = 1u16 << (15 - n.leading_zeros() as u16);
    let mut w = Writer(Vec::new());
    w.u16(0)
        .u16(1)
        .u16(0)
        .u16(14 + 6 * n)
        .u16(1)
        .u16(n)
        .u16(6 * power)
        .u16(power.trailing_zeros() as u16)
        .u16(6 * n - 6 * power);
    for (l, r, v) in pairs {
        w.u16(l).u16(r).i16(v);
    }
    Ok(w.0)
}

/// The sum of a table's big-endian 32-bit words, padded with zeros.
pub(crate) fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn i16(&mut self, v: i32) -> &mut Self {
        self.u16(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16)
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn i64(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
}

fn bounds_of_all(outlines: &[Outline]) -> (i32, i32, i32, i32) {
    outlines
        .iter()
        .filter_map(Outline::bounds)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
        .unwrap_or_default()
}

fn head(font: &Font, outlines: &[Outline], u: i32, version: (u16, u16)) -> Vec<u8> {
    let (x0, y0, x1, y1) = bounds_of_all(outlines);
    let mut w = Writer(Vec::new());
    w.u32(0x0001_0000)
        .u32(revision(version))
        .u32(0) // checkSumAdjustment, set once the file is assembled
        .u32(0x5f0f_3cf5)
        .u16(0b1011) // baseline at y=0, left side bearing at x=0, integer scaling
        .u16((font.height as i32 * u) as u16)
        .i64(0) // created: no time, for reproducible files
        .i64(0) // modified
        .i16(x0)
        .i16(y0)
        .i16(x1)
        .i16(y1)
        .u16(0) // macStyle
        .u16(font.height as u16) // lowestRecPPEM
        .i16(2) // fontDirectionHint
        .i16(1) // indexToLocFormat: long offsets
        .i16(0); // glyphDataFormat
    w.0
}

/// `major.minor` as 16.16 fixed point, the minor part in thousandths.
fn revision((major, minor): (u16, u16)) -> u32 {
    ((major as u32) << 16) + (minor.min(999) as u32 * 65536 + 500) / 1000
}

fn hhea(font: &Font, outlines: &[Outline], u: i32) -> Vec<u8> {
    let ascent = font.baseline as i32 * u;
    let descent = -((font.height - font.baseline) as i32) * u;
    let gap = (font.line_height as i32 - font.height as i32).max(0) * u;
    let advance_max = outlines.iter().map(|o| o.advance).max().unwrap_or(0);
    let drawn: Vec<(i32, i32, i32, i32, i32)> = outlines
        .iter()
        .filter_map(|o| {
            o.bounds()
                .map(|(x0, _, x1, _)| (x0, x1, o.advance, x0, o.advance - x1))
        })
        .collect();
    let min_lsb = drawn.iter().map(|d| d.3).min().unwrap_or(0);
    let min_rsb = drawn.iter().map(|d| d.4).min().unwrap_or(0);
    let extent = drawn.iter().map(|d| d.1).max().unwrap_or(0);
    let mut w = Writer(Vec::new());
    w.u32(0x0001_0000)
        .i16(ascent)
        .i16(descent)
        .i16(gap)
        .u16(advance_max as u16)
        .i16(min_lsb)
        .i16(min_rsb)
        .i16(extent)
        .i16(1) // caretSlopeRise
        .i16(0)
        .i16(0)
        .i16(0)
        .i16(0)
        .i16(0)
        .i16(0)
        .i16(0) // metricDataFormat
        .u16(outlines.len() as u16);
    w.0
}

fn hmtx(outlines: &[Outline]) -> Vec<u8> {
    let mut w = Writer(Vec::new());
    for o in outlines {
        w.u16(o.advance as u16).i16(o.bounds().map_or(0, |b| b.0));
    }
    w.0
}

fn maxp(outlines: &[Outline]) -> Vec<u8> {
    let points = outlines
        .iter()
        .map(|o| o.contours.iter().map(Vec::len).sum::<usize>())
        .max()
        .unwrap_or(0);
    let contours = outlines.iter().map(|o| o.contours.len()).max().unwrap_or(0);
    let mut w = Writer(Vec::new());
    w.u32(0x0001_0000)
        .u16(outlines.len() as u16)
        .u16(points as u16)
        .u16(contours as u16)
        .u16(0) // maxCompositePoints
        .u16(0) // maxCompositeContours
        .u16(2) // maxZones
        .u16(0)
        .u16(0)
        .u16(0)
        .u16(0)
        .u16(0)
        .u16(0)
        .u16(0)
        .u16(0); // maxComponentDepth
    w.0
}

fn glyf_and_loca(outlines: &[Outline]) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Writer(Vec::new());
    let mut loca = Writer(Vec::new());
    for o in outlines {
        loca.u32(glyf.0.len() as u32);
        let Some((x0, y0, x1, y1)) = o.bounds() else {
            continue;
        };
        glyf.i16(o.contours.len() as i32)
            .i16(x0)
            .i16(y0)
            .i16(x1)
            .i16(y1);
        let mut end = 0;
        for c in &o.contours {
            end += c.len();
            glyf.u16(end as u16 - 1);
        }
        glyf.u16(0); // no instructions
        for _ in 0..end {
            glyf.u8(1); // on the curve: every point is a corner
        }
        let points: Vec<(i32, i32)> = o.contours.iter().flatten().copied().collect();
        let mut previous = 0;
        for &(x, _) in &points {
            glyf.i16(x - previous);
            previous = x;
        }
        previous = 0;
        for &(_, y) in &points {
            glyf.i16(y - previous);
            previous = y;
        }
        glyf.0.resize(glyf.0.len().div_ceil(4) * 4, 0);
    }
    loca.u32(glyf.0.len() as u32);
    (glyf.0, loca.0)
}

fn cmap(glyphs: &[Glyph]) -> Vec<u8> {
    // Characters with their glyph ids, ascending; glyph 0 is .notdef.
    let mut map: Vec<(u32, u32)> = glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| (g.ch as u32, i as u32 + 1))
        .collect();
    map.sort_unstable();
    let bmp: Vec<(u32, u32)> = map.iter().copied().filter(|&(c, _)| c <= 0xfffe).collect();
    let wide = map.iter().any(|&(c, _)| c > 0xffff);

    // Format 4: runs of consecutive characters with consecutive glyph ids.
    let mut segments: Vec<(u32, u32, u32)> = Vec::new(); // (start, end, first glyph)
    for &(c, g) in &bmp {
        match segments.last_mut() {
            Some(s) if s.1 + 1 == c && s.2 + (c - s.0) == g => s.1 = c,
            _ => segments.push((c, c, g)),
        }
    }
    segments.push((0xffff, 0xffff, 0));
    let n = segments.len() as u16;
    let power = 1u16 << (15 - n.leading_zeros() as u16);
    let mut f4 = Writer(Vec::new());
    f4.u16(4)
        .u16(16 + 8 * n)
        .u16(0)
        .u16(n * 2)
        .u16(power * 2)
        .u16(power.trailing_zeros() as u16)
        .u16(n * 2 - power * 2);
    for s in &segments {
        f4.u16(s.1 as u16);
    }
    f4.u16(0);
    for s in &segments {
        f4.u16(s.0 as u16);
    }
    for s in &segments {
        let delta = if s.0 == 0xffff {
            1
        } else {
            s.2.wrapping_sub(s.0)
        };
        f4.u16(delta as u16);
    }
    for _ in &segments {
        f4.u16(0);
    }
    // Format 12, when a character is outside the Basic Multilingual Plane.
    let mut f12 = Writer(Vec::new());
    if wide {
        let mut groups: Vec<(u32, u32, u32)> = Vec::new();
        for &(c, g) in &map {
            match groups.last_mut() {
                Some(s) if s.1 + 1 == c && s.2 + (c - s.0) == g => s.1 = c,
                _ => groups.push((c, c, g)),
            }
        }
        f12.u16(12)
            .u16(0)
            .u32(16 + 12 * groups.len() as u32)
            .u32(0)
            .u32(groups.len() as u32);
        for (start, end, glyph) in groups {
            f12.u32(start).u32(end).u32(glyph);
        }
    }
    let records: Vec<(u16, u16, bool)> = if wide {
        vec![(0, 3, false), (0, 4, true), (3, 1, false), (3, 10, true)]
    } else {
        vec![(0, 3, false), (3, 1, false)]
    };
    let header = 4 + 8 * records.len() as u32;
    let mut w = Writer(Vec::new());
    w.u16(0).u16(records.len() as u16);
    for (platform, encoding, is_12) in &records {
        let offset = if *is_12 {
            header + f4.0.len() as u32
        } else {
            header
        };
        w.u16(*platform).u16(*encoding).u32(offset);
    }
    w.0.extend_from_slice(&f4.0);
    w.0.extend_from_slice(&f12.0);
    w.0
}

fn name(family: &str, version: (u16, u16)) -> Vec<u8> {
    let version_text = format!("Version {}.{:03}", version.0, version.1.min(999));
    let postscript: String = family
        .chars()
        .filter(|c| c.is_ascii_graphic() && !"[](){}<>/%".contains(*c))
        .take(63)
        .collect();
    let strings = [
        (1u16, family.to_owned()),
        (2, "Regular".to_owned()),
        (3, format!("spritebox: {family}: {version_text}")),
        (4, family.to_owned()),
        (5, version_text),
        (6, postscript),
    ];
    let encoded: Vec<(u16, Vec<u8>)> = strings
        .iter()
        .map(|(id, s)| {
            (
                *id,
                s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect(),
            )
        })
        .collect();
    let mut w = Writer(Vec::new());
    w.u16(0)
        .u16(encoded.len() as u16)
        .u16(6 + 12 * encoded.len() as u16);
    let mut offset = 0u16;
    for (id, bytes) in &encoded {
        w.u16(3)
            .u16(1)
            .u16(0x0409)
            .u16(*id)
            .u16(bytes.len() as u16)
            .u16(offset);
        offset += bytes.len() as u16;
    }
    for (_, bytes) in &encoded {
        w.0.extend_from_slice(bytes);
    }
    w.0
}

fn os2(font: &Font, outlines: &[Outline], u: i32) -> Vec<u8> {
    let ascent = font.baseline as i32 * u;
    let descent = -((font.height - font.baseline) as i32) * u;
    let gap = (font.line_height as i32 - font.height as i32).max(0) * u;
    let (_, y0, _, y1) = bounds_of_all(outlines);
    let advances: Vec<i32> = outlines
        .iter()
        .map(|o| o.advance)
        .filter(|&a| a > 0)
        .collect();
    let average = if advances.is_empty() {
        0
    } else {
        advances.iter().sum::<i32>() / advances.len() as i32
    };
    let chars: Vec<u32> = font.glyphs.iter().map(|g| g.ch as u32).collect();
    let first = chars.iter().min().copied().unwrap_or(0).min(0xffff);
    let last = chars.iter().max().copied().unwrap_or(0).min(0xffff);
    let mut ranges = 0u32;
    if chars.iter().any(|&c| (0x20..0x7f).contains(&c)) {
        ranges |= 1; // Basic Latin
    }
    if chars.iter().any(|&c| (0xa0..0x100).contains(&c)) {
        ranges |= 2; // Latin-1 Supplement
    }
    // The top of a glyph's ink, for x-height and cap height.
    let top = |c: char| {
        font.glyphs
            .iter()
            .position(|g| g.ch == c)
            .and_then(|i| outlines[i + 1].bounds())
            .map_or(0, |b| b.3)
    };
    let em = font.height as i32 * u;
    let mut w = Writer(Vec::new());
    w.u16(4)
        .i16(average)
        .u16(400) // regular weight
        .u16(5) // normal width
        .u16(0) // installable embedding
        .i16(em * 65 / 100)
        .i16(em * 60 / 100)
        .i16(0)
        .i16(em * 7 / 100)
        .i16(em * 65 / 100)
        .i16(em * 60 / 100)
        .i16(0)
        .i16(em * 48 / 100)
        .i16(u) // strikeout size: one pixel
        .i16(ascent / 2) // strikeout position
        .i16(0); // sFamilyClass
    w.0.extend_from_slice(&[0; 10]); // PANOSE: any
    w.u32(ranges).u32(0).u32(0).u32(0);
    w.0.extend_from_slice(b"NONE");
    w.u16(0x0040 | 0x0080) // regular, use typographic metrics
        .u16(first as u16)
        .u16(last as u16)
        .i16(ascent)
        .i16(descent)
        .i16(gap)
        .u16(ascent.max(y1).max(0) as u16)
        .u16((-descent).max(-y0).max(0) as u16)
        .u32(1) // code page: Latin 1
        .u32(0)
        .i16(top('x'))
        .i16(top('H'))
        .u16(0) // default char: .notdef
        .u16(0x20) // break char: space
        .u16(1); // no context beyond one character
    w.0
}

fn post(outlines: &[Outline], u: i32) -> Vec<u8> {
    let advances: Vec<i32> = outlines.iter().skip(1).map(|o| o.advance).collect();
    let fixed = advances.windows(2).all(|w| w[0] == w[1]);
    let mut w = Writer(Vec::new());
    w.u32(0x0003_0000) // no glyph names
        .u32(0) // italic angle
        .i16(-u) // underline position
        .i16(u) // underline thickness
        .u32(fixed as u32)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0);
    w.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        assert_eq!(checksum(&[0, 0, 0, 1, 0, 0, 0, 2]), 3);
        assert_eq!(checksum(&[1]), 0x0100_0000);
        assert_eq!(revision((1, 0)), 0x0001_0000);
        assert_eq!(revision((2, 500)), 0x0002_8000);
    }
}
