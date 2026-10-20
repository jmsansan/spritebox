//! PNG encoding and decoding, with the crate's own DEFLATE, zlib, CRC-32 and Adler-32.
//!
//! [`encode`] writes the smallest of two lossless forms: an indexed image (bit depth 1,
//! 2, 4 or 8, with a `tRNS` chunk for colors that are not opaque) when the image has at
//! most 256 distinct RGBA colors, which is usual for pixel art, and 8-bit RGBA
//! otherwise. Rows of RGBA are filtered with the filter that minimizes the sum of
//! absolute differences. The file holds no time stamp or other metadata, so the same
//! image always gives the same bytes.
//!
//! [`decode`] reads every standard PNG: grayscale, RGB, indexed, grayscale with alpha
//! and RGBA, at every bit depth, interlaced or not, with `tRNS` transparency. It
//! returns RGBA8: 16-bit samples are rounded to 8 bits, and gamma and color-space
//! chunks are ignored. CRCs are checked, and unknown critical chunks are rejected.
//!
//! ```
//! use spritebox::{Color, Image, png};
//!
//! let mut image = Image::new(2, 1)?;
//! image.set_pixel(0, 0, Color::rgb(255, 0, 0));
//! let bytes = png::encode(&image);
//! assert_eq!(png::decode(&bytes)?, image);
//! # Ok::<(), spritebox::Error>(())
//! ```

mod checksum;
mod deflate;

use std::collections::HashMap;
use std::path::Path;

use crate::{Error, ErrorCode, Image, Limits, Result};
use checksum::{Crc32, crc32};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// The largest IDAT chunk written; larger data is split over several chunks.
const IDAT_SIZE: usize = 1 << 20;

/// Encodes `image` as a PNG file.
pub fn encode(image: &Image) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    let (width, height) = (image.width(), image.height());
    let palette = palette_of(image);
    let (header_depth, color_type, raw) = match &palette {
        Some((colors, indices)) => {
            let depth = match colors.len() {
                0..=2 => 1,
                3..=4 => 2,
                5..=16 => 4,
                _ => 8,
            };
            (depth, 3, indexed_rows(image, indices, depth))
        }
        None => (8, 6, filtered_rgba_rows(image)),
    };
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[header_depth, color_type, 0, 0, 0]);
    write_chunk(&mut out, b"IHDR", &header);
    if let Some((colors, _)) = &palette {
        let plte: Vec<u8> = colors.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        write_chunk(&mut out, b"PLTE", &plte);
        let alphas = colors
            .iter()
            .rposition(|c| c[3] != 255)
            .map_or(0, |i| i + 1);
        if alphas > 0 {
            let trns: Vec<u8> = colors[..alphas].iter().map(|c| c[3]).collect();
            write_chunk(&mut out, b"tRNS", &trns);
        }
    }
    let compressed = deflate::zlib_compress(&raw);
    for chunk in compressed.chunks(IDAT_SIZE) {
        write_chunk(&mut out, b"IDAT", chunk);
    }
    write_chunk(&mut out, b"IEND", &[]);
    out
}

/// Encodes `image` and writes it to `path`.
pub fn write_file(path: impl AsRef<Path>, image: &Image) -> Result<()> {
    std::fs::write(path, encode(image))?;
    Ok(())
}

/// Decodes a PNG file to RGBA8, within the default [`Limits`].
pub fn decode(bytes: &[u8]) -> Result<Image> {
    decode_with(bytes, &Limits::default())
}

/// Reads and decodes a PNG file, within the default [`Limits`].
pub fn read_file(path: impl AsRef<Path>) -> Result<Image> {
    decode(&std::fs::read(path)?)
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// The image's colors in order of first appearance, and each pixel's index, if there
/// are at most 256 colors.
fn palette_of(image: &Image) -> Option<(Vec<[u8; 4]>, Vec<u8>)> {
    let mut colors = Vec::new();
    let mut lookup: HashMap<[u8; 4], u8> = HashMap::new();
    let mut indices = Vec::with_capacity(image.pixels().len() / 4);
    for p in image.pixels().chunks_exact(4) {
        let color = [p[0], p[1], p[2], p[3]];
        let index = match lookup.get(&color) {
            Some(&i) => i,
            None => {
                if colors.len() == 256 {
                    return None;
                }
                let i = colors.len() as u8;
                colors.push(color);
                lookup.insert(color, i);
                i
            }
        };
        indices.push(index);
    }
    Some((colors, indices))
}

fn indexed_rows(image: &Image, indices: &[u8], depth: u8) -> Vec<u8> {
    let width = image.width() as usize;
    let per_byte = 8 / depth as usize;
    let stride = width.div_ceil(per_byte);
    let mut raw = Vec::with_capacity((stride + 1) * image.height() as usize);
    for row in indices.chunks_exact(width) {
        raw.push(0);
        for group in row.chunks(per_byte) {
            let mut byte = 0u8;
            for (k, &index) in group.iter().enumerate() {
                byte |= index << (8 - depth as usize * (k + 1));
            }
            raw.push(byte);
        }
    }
    raw
}

fn filtered_rgba_rows(image: &Image) -> Vec<u8> {
    let stride = image.width() as usize * 4;
    let zero = vec![0u8; stride];
    let mut raw = Vec::with_capacity((stride + 1) * image.height() as usize);
    let mut candidate = vec![0u8; stride];
    let mut best = vec![0u8; stride];
    let mut previous: &[u8] = &zero;
    for row in image.pixels().chunks_exact(stride) {
        let mut best_filter = 0;
        let mut best_score = u64::MAX;
        for filter in 0..5u8 {
            for i in 0..stride {
                let a = if i >= 4 { row[i - 4] } else { 0 };
                let b = previous[i];
                let c = if i >= 4 { previous[i - 4] } else { 0 };
                candidate[i] = row[i].wrapping_sub(predict(filter, a, b, c));
            }
            let score: u64 = candidate
                .iter()
                .map(|&v| (v as i8).unsigned_abs() as u64)
                .sum();
            if score < best_score {
                best_score = score;
                best_filter = filter;
                std::mem::swap(&mut best, &mut candidate);
            }
        }
        raw.push(best_filter);
        raw.extend_from_slice(&best);
        previous = row;
    }
    raw
}

fn predict(filter: u8, a: u8, b: u8, c: u8) -> u8 {
    match filter {
        1 => a,
        2 => b,
        3 => ((a as u16 + b as u16) / 2) as u8,
        4 => {
            let p = a as i16 + b as i16 - c as i16;
            let (pa, pb, pc) = (
                (p - a as i16).abs(),
                (p - b as i16).abs(),
                (p - c as i16).abs(),
            );
            if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            }
        }
        _ => 0,
    }
}

fn malformed(message: impl std::fmt::Display) -> Error {
    Error::decode(format!("Malformed PNG: {message}"))
}

struct Header {
    width: u32,
    height: u32,
    depth: u8,
    color_type: u8,
    interlaced: bool,
}

impl Header {
    fn channels(&self) -> usize {
        match self.color_type {
            0 | 3 => 1,
            4 => 2,
            2 => 3,
            _ => 4,
        }
    }
    fn bits_per_pixel(&self) -> usize {
        self.channels() * self.depth as usize
    }
    fn stride(&self, width: u32) -> usize {
        (width as usize * self.bits_per_pixel()).div_ceil(8)
    }
}

/// Adam7 passes as `(x0, y0, dx, dy)`.
const ADAM7: [(u32, u32, u32, u32); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];

/// The passes of an image as `(x0, y0, dx, dy, width, height)`, without empty ones.
fn passes(header: &Header) -> Vec<(u32, u32, u32, u32, u32, u32)> {
    if !header.interlaced {
        return vec![(0, 0, 1, 1, header.width, header.height)];
    }
    ADAM7
        .iter()
        .map(|&(x0, y0, dx, dy)| {
            let w = header.width.saturating_sub(x0).div_ceil(dx);
            let h = header.height.saturating_sub(y0).div_ceil(dy);
            (x0, y0, dx, dy, w, h)
        })
        .filter(|p| p.4 > 0 && p.5 > 0)
        .collect()
}

/// Decodes a PNG file to RGBA8, within `limits`.
pub fn decode_with(bytes: &[u8], limits: &Limits) -> Result<Image> {
    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        return Err(Error::new(ErrorCode::UnsupportedFormat, "Not a PNG file"));
    }
    let mut pos = 8;
    let mut header: Option<Header> = None;
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut transparency: Option<Vec<u8>> = None;
    let mut data = Vec::new();
    let mut seen_idat = false;
    let mut ended = false;
    while !ended {
        if bytes.len() - pos < 12 {
            return Err(malformed("the file ends inside a chunk"));
        }
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let kind: [u8; 4] = [
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ];
        if len > bytes.len() - pos - 12 {
            return Err(malformed("the file ends inside a chunk"));
        }
        let body = &bytes[pos + 8..pos + 8 + len];
        let stored = &bytes[pos + 8 + len..pos + 12 + len];
        if crc32(&bytes[pos + 4..pos + 8 + len])
            != u32::from_be_bytes([stored[0], stored[1], stored[2], stored[3]])
        {
            return Err(malformed(format!(
                "the CRC of chunk {} does not match",
                chunk_name(kind)
            )));
        }
        pos += 12 + len;
        if header.is_none() && &kind != b"IHDR" {
            return Err(malformed("the first chunk is not IHDR"));
        }
        match &kind {
            b"IHDR" => {
                if header.is_some() {
                    return Err(malformed("two IHDR chunks"));
                }
                header = Some(read_header(body, limits)?);
            }
            b"PLTE" => {
                if body.len() % 3 != 0 || body.is_empty() || body.len() > 768 {
                    return Err(malformed("a PLTE chunk of the wrong length"));
                }
                palette = body
                    .chunks_exact(3)
                    .map(|c| [c[0], c[1], c[2], 255])
                    .collect();
            }
            b"tRNS" => transparency = Some(body.to_vec()),
            b"IDAT" => {
                seen_idat = true;
                data.extend_from_slice(body);
            }
            b"IEND" => ended = true,
            _ if kind[0] & 0x20 == 0 => {
                return Err(Error::new(
                    ErrorCode::UnsupportedFormat,
                    format!("Unsupported critical PNG chunk {}", chunk_name(kind)),
                ));
            }
            _ => {}
        }
    }
    let header = header.ok_or_else(|| malformed("no IHDR chunk"))?;
    if !seen_idat {
        return Err(malformed("no IDAT chunk"));
    }
    if header.color_type == 3 {
        if palette.is_empty() {
            return Err(malformed("an indexed image without a PLTE chunk"));
        }
        if let Some(alphas) = &transparency {
            if alphas.len() > palette.len() {
                return Err(malformed("more tRNS entries than palette colors"));
            }
            for (color, &a) in palette.iter_mut().zip(alphas) {
                color[3] = a;
            }
        }
    }
    let passes = passes(&header);
    let expected: usize = passes
        .iter()
        .map(|p| (header.stride(p.4) + 1) * p.5 as usize)
        .sum();
    let raw = deflate::zlib_decompress(&data, expected)?;
    if raw.len() != expected {
        return Err(malformed("the image data ends early"));
    }
    let key = transparency
        .as_deref()
        .filter(|_| matches!(header.color_type, 0 | 2));
    let mut image = Image::blank(header.width, header.height);
    let mut offset = 0;
    for &(x0, y0, dx, dy, w, h) in &passes {
        let stride = header.stride(w);
        let bpp = header.bits_per_pixel().div_ceil(8);
        let mut previous = vec![0u8; stride];
        for row in 0..h {
            let filter = raw[offset];
            let line = &raw[offset + 1..offset + 1 + stride];
            offset += stride + 1;
            let mut current = vec![0u8; stride];
            for i in 0..stride {
                let a = if i >= bpp { current[i - bpp] } else { 0 };
                let c = if i >= bpp { previous[i - bpp] } else { 0 };
                if filter > 4 {
                    return Err(malformed(format!("unknown filter type {filter}")));
                }
                current[i] = line[i].wrapping_add(predict(filter, a, previous[i], c));
            }
            for col in 0..w {
                let color = pixel(&header, &current, col as usize, &palette, key)?;
                image.set_pixel(
                    x0 + col * dx,
                    y0 + row * dy,
                    crate::Color::from_array(color),
                );
            }
            previous = current;
        }
    }
    Ok(image)
}

fn chunk_name(kind: [u8; 4]) -> String {
    kind.iter()
        .map(|&b| {
            if b.is_ascii_alphabetic() {
                b as char
            } else {
                '?'
            }
        })
        .collect()
}

fn read_header(body: &[u8], limits: &Limits) -> Result<Header> {
    if body.len() != 13 {
        return Err(malformed("an IHDR chunk of the wrong length"));
    }
    let width = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
    let height = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
    let header = Header {
        width,
        height,
        depth: body[8],
        color_type: body[9],
        interlaced: body[12] == 1,
    };
    let depth_ok = match header.color_type {
        0 => matches!(header.depth, 1 | 2 | 4 | 8 | 16),
        3 => matches!(header.depth, 1 | 2 | 4 | 8),
        2 | 4 | 6 => matches!(header.depth, 8 | 16),
        _ => {
            return Err(malformed(format!(
                "unknown color type {}",
                header.color_type
            )));
        }
    };
    if !depth_ok {
        return Err(malformed(format!(
            "bit depth {} is not allowed for color type {}",
            header.depth, header.color_type
        )));
    }
    if body[10] != 0 || body[11] != 0 || body[12] > 1 {
        return Err(malformed("unknown compression, filter or interlace method"));
    }
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(malformed(format!("invalid size {width}x{height}")));
    }
    limits.check_image(width, height)?;
    Ok(header)
}

/// One pixel of an unfiltered row, as RGBA8.
fn pixel(
    header: &Header,
    row: &[u8],
    x: usize,
    palette: &[[u8; 4]],
    key: Option<&[u8]>,
) -> Result<[u8; 4]> {
    let depth = header.depth as usize;
    let sample = |channel: usize| -> u16 {
        let bit = (x * header.channels() + channel) * depth;
        match depth {
            16 => u16::from_be_bytes([row[bit / 8], row[bit / 8 + 1]]),
            8 => row[bit / 8] as u16,
            _ => ((row[bit / 8] >> (8 - depth - bit % 8)) & ((1 << depth) - 1) as u8) as u16,
        }
    };
    let to8 = |v: u16| -> u8 {
        match depth {
            16 => ((v as u32 * 255 + 32767) / 65535) as u8,
            8 => v as u8,
            _ => (v as u32 * 255 / ((1 << depth) - 1)) as u8,
        }
    };
    let keyed = |samples: &[u16]| -> bool {
        key.is_some_and(|k| {
            k.len() >= samples.len() * 2
                && samples
                    .iter()
                    .enumerate()
                    .all(|(i, &s)| u16::from_be_bytes([k[2 * i], k[2 * i + 1]]) == s)
        })
    };
    Ok(match header.color_type {
        0 => {
            let g = sample(0);
            let v = to8(g);
            [v, v, v, if keyed(&[g]) { 0 } else { 255 }]
        }
        2 => {
            let (r, g, b) = (sample(0), sample(1), sample(2));
            [
                to8(r),
                to8(g),
                to8(b),
                if keyed(&[r, g, b]) { 0 } else { 255 },
            ]
        }
        3 => *palette
            .get(sample(0) as usize)
            .ok_or_else(|| malformed("a palette index past the end of the palette"))?,
        4 => {
            let v = to8(sample(0));
            [v, v, v, to8(sample(1))]
        }
        _ => [
            to8(sample(0)),
            to8(sample(1)),
            to8(sample(2)),
            to8(sample(3)),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

    #[test]
    fn indexed_depths_round_trip() {
        for colors in [1usize, 2, 3, 4, 5, 16, 17, 256, 257] {
            let mut image = Image::new(37, 9).unwrap();
            for i in 0..37 * 9u32 {
                let k = (i as usize * 7) % colors;
                image.set_pixel(
                    i % 37,
                    i / 37,
                    Color::rgba(
                        k as u8,
                        (k >> 8) as u8 * 50,
                        3,
                        if k % 3 == 0 { 255 } else { k as u8 },
                    ),
                );
            }
            let bytes = encode(&image);
            assert_eq!(decode(&bytes).unwrap(), image, "{colors} colors");
            let color_type = bytes[25];
            assert_eq!(color_type, if colors > 256 { 6 } else { 3 });
        }
    }
}
