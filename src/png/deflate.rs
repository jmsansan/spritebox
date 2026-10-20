//! DEFLATE (RFC 1951) inside the zlib wrapper (RFC 1950): the compressor that PNG
//! writing uses and the decompressor that PNG reading uses.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::checksum::adler32;
use crate::{Error, ErrorCode, Result};

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order in which code-length code lengths are stored.
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const END_OF_BLOCK: usize = 256;

fn fixed_literal_lengths() -> [u8; 288] {
    let mut lengths = [8u8; 288];
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    lengths
}

// ---------------------------------------------------------------------------
// Decompression
// ---------------------------------------------------------------------------

fn corrupt(message: &str) -> Error {
    Error::decode(format!("Malformed zlib data: {message}"))
}

/// Reads bits least significant first, as DEFLATE stores them.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bits: u64,
    count: u32,
    /// Zero bits appended past the end of `data`, still in `bits`.
    padding: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bits: 0,
            count: 0,
            padding: 0,
        }
    }
    fn refill(&mut self) {
        while self.count <= 56 {
            let byte = match self.data.get(self.pos) {
                Some(&b) => b,
                None => {
                    self.padding += 8;
                    0
                }
            };
            self.pos += 1;
            self.bits |= (byte as u64) << self.count;
            self.count += 8;
        }
    }
    fn peek(&mut self, n: u32) -> u32 {
        if self.count < n {
            self.refill();
        }
        (self.bits & ((1u64 << n) - 1)) as u32
    }
    fn consume(&mut self, n: u32) -> Result<()> {
        self.bits >>= n;
        self.count -= n;
        if self.padding > self.count {
            return Err(corrupt("the stream ends early"));
        }
        Ok(())
    }
    fn bits(&mut self, n: u32) -> Result<u32> {
        let value = self.peek(n);
        self.consume(n)?;
        Ok(value)
    }
    fn align(&mut self) -> Result<()> {
        self.consume(self.count % 8)
    }
    /// Bytes of input consumed so far, once aligned.
    fn byte_position(&self) -> usize {
        self.pos - (self.count / 8) as usize
    }
    /// Copies `n` bytes after aligning to a byte.
    fn copy_bytes(&mut self, mut n: usize, out: &mut Vec<u8>) -> Result<()> {
        while n > 0 && self.count >= 8 {
            out.push(self.bits(8)? as u8);
            n -= 1;
        }
        if n > self.data.len().saturating_sub(self.pos) {
            return Err(corrupt("a stored block ends early"));
        }
        out.extend_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(())
    }
}

/// A canonical Huffman code decoded with one table lookup.
struct Decoder {
    /// Indexed by the next `max_len` bits: `symbol << 4 | length`, or 0 for no code.
    table: Vec<u16>,
    max_len: u32,
}

impl Decoder {
    fn new(lengths: &[u8]) -> Result<Self> {
        let mut count = [0u32; 16];
        for &len in lengths {
            count[len as usize] += 1;
        }
        count[0] = 0;
        let mut left = 1i64;
        for &c in &count[1..] {
            left = (left << 1) - c as i64;
            if left < 0 {
                return Err(corrupt("a Huffman code is over-subscribed"));
            }
        }
        let max_len = (1..16).rev().find(|&l| count[l] > 0).unwrap_or(1) as u32;
        let mut next = [0u32; 16];
        let mut code = 0u32;
        for len in 1..16 {
            code = (code + count[len - 1]) << 1;
            next[len] = code;
        }
        let mut table = vec![0u16; 1 << max_len];
        for (symbol, &len) in lengths.iter().enumerate() {
            if len == 0 {
                continue;
            }
            let len = len as u32;
            let reversed = reverse_bits(next[len as usize], len) as usize;
            next[len as usize] += 1;
            let entry = (symbol as u16) << 4 | len as u16;
            for slot in (reversed..table.len()).step_by(1 << len) {
                table[slot] = entry;
            }
        }
        Ok(Self { table, max_len })
    }
    fn decode(&self, reader: &mut BitReader) -> Result<usize> {
        let entry = self.table[reader.peek(self.max_len) as usize];
        if entry == 0 {
            return Err(corrupt("an invalid Huffman code"));
        }
        reader.consume((entry & 15) as u32)?;
        Ok((entry >> 4) as usize)
    }
}

fn reverse_bits(code: u32, len: u32) -> u32 {
    code.reverse_bits() >> (32 - len)
}

/// Decompresses a zlib stream whose decompressed size is at most `max_output` bytes.
pub(crate) fn zlib_decompress(data: &[u8], max_output: usize) -> Result<Vec<u8>> {
    if data.len() < 2 {
        return Err(corrupt("the stream ends early"));
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 0x0f != 8 || cmf >> 4 > 7 || (cmf as u16 * 256 + flg as u16) % 31 != 0 {
        return Err(corrupt("the header is not a DEFLATE stream"));
    }
    if flg & 0x20 != 0 {
        return Err(Error::new(
            ErrorCode::UnsupportedFormat,
            "zlib streams with a preset dictionary are not supported",
        ));
    }
    let (out, used) = inflate(&data[2..], max_output)?;
    let trailer = data
        .get(2 + used..2 + used + 4)
        .ok_or_else(|| corrupt("the Adler-32 checksum is missing"))?;
    let expected = u32::from_be_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    if adler32(&out) != expected {
        return Err(corrupt("the Adler-32 checksum does not match"));
    }
    Ok(out)
}

/// Decompresses raw DEFLATE data; returns the output and the bytes of input used.
fn inflate(data: &[u8], max_output: usize) -> Result<(Vec<u8>, usize)> {
    let mut reader = BitReader::new(data);
    let mut out = Vec::new();
    let mut fixed: Option<(Decoder, Decoder)> = None;
    loop {
        let last = reader.bits(1)? == 1;
        match reader.bits(2)? {
            0 => {
                reader.align()?;
                let len = reader.bits(16)?;
                let nlen = reader.bits(16)?;
                if len != !nlen & 0xffff {
                    return Err(corrupt("a stored block's length check fails"));
                }
                if out.len() + len as usize > max_output {
                    return Err(corrupt("more data than the image needs"));
                }
                reader.copy_bytes(len as usize, &mut out)?;
            }
            1 => {
                if fixed.is_none() {
                    fixed = Some((
                        Decoder::new(&fixed_literal_lengths())?,
                        Decoder::new(&[5; 30])?,
                    ));
                }
                let (literals, distances) = fixed.as_ref().unwrap_or_else(|| unreachable!());
                inflate_block(&mut reader, literals, distances, &mut out, max_output)?;
            }
            2 => {
                let (literals, distances) = read_dynamic_codes(&mut reader)?;
                inflate_block(&mut reader, &literals, &distances, &mut out, max_output)?;
            }
            _ => return Err(corrupt("a block of reserved type 3")),
        }
        if last {
            break;
        }
    }
    reader.align()?;
    Ok((out, reader.byte_position()))
}

fn read_dynamic_codes(reader: &mut BitReader) -> Result<(Decoder, Decoder)> {
    let hlit = reader.bits(5)? as usize + 257;
    let hdist = reader.bits(5)? as usize + 1;
    let hclen = reader.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(corrupt("too many codes in a dynamic block"));
    }
    let mut cl_lengths = [0u8; 19];
    for &i in &CL_ORDER[..hclen] {
        cl_lengths[i] = reader.bits(3)? as u8;
    }
    let cl = Decoder::new(&cl_lengths)?;
    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lengths.len() {
        let symbol = cl.decode(reader)?;
        let (value, repeat) = match symbol {
            0..=15 => (symbol as u8, 1),
            16 => {
                let previous = *i
                    .checked_sub(1)
                    .and_then(|p| lengths.get(p))
                    .ok_or_else(|| corrupt("a repeat with no previous length"))?;
                (previous, 3 + reader.bits(2)? as usize)
            }
            17 => (0, 3 + reader.bits(3)? as usize),
            _ => (0, 11 + reader.bits(7)? as usize),
        };
        if i + repeat > lengths.len() {
            return Err(corrupt("code lengths overflow their table"));
        }
        lengths[i..i + repeat].fill(value);
        i += repeat;
    }
    if lengths[END_OF_BLOCK] == 0 {
        return Err(corrupt("a dynamic block has no end-of-block code"));
    }
    Ok((
        Decoder::new(&lengths[..hlit])?,
        Decoder::new(&lengths[hlit..])?,
    ))
}

fn inflate_block(
    reader: &mut BitReader,
    literals: &Decoder,
    distances: &Decoder,
    out: &mut Vec<u8>,
    max_output: usize,
) -> Result<()> {
    loop {
        let symbol = literals.decode(reader)?;
        if symbol < 256 {
            if out.len() >= max_output {
                return Err(corrupt("more data than the image needs"));
            }
            out.push(symbol as u8);
            continue;
        }
        if symbol == END_OF_BLOCK {
            return Ok(());
        }
        let index = symbol - 257;
        if index >= 29 {
            return Err(corrupt("an invalid length code"));
        }
        let length =
            LENGTH_BASE[index] as usize + reader.bits(LENGTH_EXTRA[index] as u32)? as usize;
        let d = distances.decode(reader)?;
        if d >= 30 {
            return Err(corrupt("an invalid distance code"));
        }
        let distance = DIST_BASE[d] as usize + reader.bits(DIST_EXTRA[d] as u32)? as usize;
        if distance > out.len() {
            return Err(corrupt("a distance before the start of the data"));
        }
        if out.len() + length > max_output {
            return Err(corrupt("more data than the image needs"));
        }
        let start = out.len() - distance;
        for k in 0..length {
            out.push(out[start + k]);
        }
    }
}

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

/// Writes bits least significant first.
struct BitWriter {
    out: Vec<u8>,
    bits: u64,
    count: u32,
}

impl BitWriter {
    fn write(&mut self, value: u32, n: u32) {
        self.bits |= (value as u64) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push(self.bits as u8);
            self.bits >>= 8;
            self.count -= 8;
        }
    }
    fn align(&mut self) {
        if self.count > 0 {
            self.out.push(self.bits as u8);
            self.bits = 0;
            self.count = 0;
        }
    }
}

const WINDOW: usize = 32768;
const HASH_BITS: u32 = 15;
const MAX_CHAIN: usize = 128;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// Matches at least this long are taken without looking one byte further.
const LAZY_LIMIT: usize = 32;
const NONE: u32 = u32::MAX;
const TOKENS_PER_BLOCK: usize = 32768;

/// A literal byte, or a match with its length and distance (bit 31 set).
type Token = u32;

fn match_token(length: usize, distance: usize) -> Token {
    1 << 31 | (length as u32) << 16 | distance as u32
}

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}

impl Matcher<'_> {
    fn hash(&self, i: usize) -> usize {
        let d = self.data;
        let v = (d[i] as u32) | (d[i + 1] as u32) << 8 | (d[i + 2] as u32) << 16;
        (v.wrapping_mul(0x9e37_79b1) >> (32 - HASH_BITS)) as usize
    }
    fn insert(&mut self, i: usize) {
        if i + MIN_MATCH <= self.data.len() {
            let h = self.hash(i);
            self.prev[i % WINDOW] = self.head[h];
            self.head[h] = i as u32;
        }
    }
    /// The longest earlier match for the bytes at `i`, as `(length, distance)`.
    fn find(&self, i: usize) -> (usize, usize) {
        let data = self.data;
        if i + MIN_MATCH > data.len() {
            return (0, 0);
        }
        let limit = MAX_MATCH.min(data.len() - i);
        let (mut best, mut best_distance) = (0, 0);
        let mut candidate = self.head[self.hash(i)];
        let mut chain = 0;
        while candidate != NONE && chain < MAX_CHAIN {
            let c = candidate as usize;
            if i - c > WINDOW {
                break;
            }
            if data[c + best] == data[i + best] {
                let mut len = 0;
                while len < limit && data[c + len] == data[i + len] {
                    len += 1;
                }
                if len > best {
                    best = len;
                    best_distance = i - c;
                    if len == limit {
                        break;
                    }
                }
            }
            let next = self.prev[c % WINDOW];
            if next == NONE || next as usize >= c {
                break;
            }
            candidate = next;
            chain += 1;
        }
        // A 3-byte match far away costs more than three literals.
        if best < MIN_MATCH || (best == MIN_MATCH && best_distance > 4096) {
            return (0, 0);
        }
        (best, best_distance)
    }
}

fn lz77(data: &[u8]) -> Vec<Token> {
    let mut matcher = Matcher {
        data,
        head: vec![NONE; 1 << HASH_BITS],
        prev: vec![NONE; WINDOW],
    };
    let mut tokens = Vec::with_capacity(data.len() / 2);
    let mut pending = None;
    let mut i = 0;
    while i < data.len() {
        let (length, distance) = pending.take().unwrap_or_else(|| matcher.find(i));
        matcher.insert(i);
        if length == 0 {
            tokens.push(data[i] as Token);
            i += 1;
            continue;
        }
        if length < LAZY_LIMIT && i + 1 < data.len() {
            let next = matcher.find(i + 1);
            if next.0 > length {
                tokens.push(data[i] as Token);
                i += 1;
                pending = Some(next);
                continue;
            }
        }
        tokens.push(match_token(length, distance));
        for j in i + 1..i + length {
            matcher.insert(j);
        }
        i += length;
    }
    tokens
}

fn length_index(length: usize) -> usize {
    LENGTH_BASE.partition_point(|&b| b as usize <= length) - 1
}

fn distance_index(distance: usize) -> usize {
    DIST_BASE.partition_point(|&b| b as usize <= distance) - 1
}

/// Code lengths of a Huffman code for `freqs`, at most `max_bits` long, complete
/// (they satisfy the Kraft equality) and with at least two codes.
fn huffman_lengths(freqs: &[u32], max_bits: u8) -> Vec<u8> {
    let mut lengths = vec![0u8; freqs.len()];
    let symbols: Vec<usize> = (0..freqs.len()).filter(|&s| freqs[s] > 0).collect();
    if symbols.len() < 2 {
        let a = symbols.first().copied().unwrap_or(0);
        lengths[a] = 1;
        lengths[if a == 0 { 1 } else { 0 }] = 1;
        return lengths;
    }
    // Build the tree; ties go to the earliest node, so the result is deterministic.
    let mut heap = BinaryHeap::new();
    let mut parent = Vec::with_capacity(symbols.len() * 2);
    for (node, &s) in symbols.iter().enumerate() {
        heap.push(Reverse((freqs[s] as u64, node)));
        parent.push(usize::MAX);
    }
    while heap.len() > 1 {
        let Reverse((wa, a)) = heap.pop().unwrap_or_default();
        let Reverse((wb, b)) = heap.pop().unwrap_or_default();
        let node = parent.len();
        parent.push(usize::MAX);
        parent[a] = node;
        parent[b] = node;
        heap.push(Reverse((wa + wb, node)));
    }
    // Parents come after their children, so depths fill from the root down.
    let mut depth = vec![0u32; parent.len()];
    for node in (0..parent.len() - 1).rev() {
        depth[node] = depth[parent[node]] + 1;
    }
    let max = max_bits as u32;
    for (node, &s) in symbols.iter().enumerate() {
        lengths[s] = depth[node].min(max) as u8;
    }
    // Clamping may over-subscribe the code: lengthen the longest codes below the
    // maximum, rarest first, then shorten the longest codes, commonest first, until
    // the code is complete again.
    let target = 1u64 << max;
    let mut kraft: u64 = symbols
        .iter()
        .map(|&s| 1u64 << (max - lengths[s] as u32))
        .sum();
    while kraft > target {
        let s = *symbols
            .iter()
            .filter(|&&s| (lengths[s] as u32) < max)
            .max_by_key(|&&s| (lengths[s], Reverse(freqs[s])))
            .unwrap_or(&symbols[0]);
        lengths[s] += 1;
        kraft -= 1u64 << (max - lengths[s] as u32);
    }
    while kraft < target {
        let s = *symbols
            .iter()
            .max_by_key(|&&s| (lengths[s], freqs[s]))
            .unwrap_or(&symbols[0]);
        kraft += 1u64 << (max - lengths[s] as u32);
        lengths[s] -= 1;
    }
    lengths
}

/// Canonical codes for `lengths`, bit-reversed for writing least significant first.
fn canonical_codes(lengths: &[u8]) -> Vec<u32> {
    let mut count = [0u32; 16];
    for &len in lengths {
        count[len as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0;
    for len in 1..16 {
        code = (code + count[len - 1]) << 1;
        next[len] = code;
    }
    lengths
        .iter()
        .map(|&len| {
            if len == 0 {
                return 0;
            }
            let c = next[len as usize];
            next[len as usize] += 1;
            reverse_bits(c, len as u32)
        })
        .collect()
}

/// Code lengths run-length encoded with the symbols 16, 17 and 18, as
/// `(symbol, extra bits value)`.
fn run_length_encode(lengths: &[u8]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lengths.len() {
        let value = lengths[i];
        let mut run = lengths[i..].iter().take_while(|&&l| l == value).count();
        i += run;
        if value == 0 {
            while run >= 11 {
                let r = run.min(138);
                out.push((18, (r - 11) as u32));
                run -= r;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u32));
                run = 0;
            }
        } else {
            out.push((value as usize, 0));
            run -= 1;
            while run >= 3 {
                let r = run.min(6);
                out.push((16, (r - 3) as u32));
                run -= r;
            }
        }
        out.extend(std::iter::repeat_n((value as usize, 0), run));
    }
    out
}

fn extra_bits(symbol: usize) -> u32 {
    match symbol {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

/// The Huffman codes of one block.
struct Codes {
    literal_lengths: Vec<u8>,
    literal_codes: Vec<u32>,
    distance_lengths: Vec<u8>,
    distance_codes: Vec<u32>,
}

impl Codes {
    fn new(literal_lengths: Vec<u8>, distance_lengths: Vec<u8>) -> Self {
        Self {
            literal_codes: canonical_codes(&literal_lengths),
            distance_codes: canonical_codes(&distance_lengths),
            literal_lengths,
            distance_lengths,
        }
    }
    fn data_bits(&self, tokens: &[Token]) -> u64 {
        let mut bits = self.literal_lengths[END_OF_BLOCK] as u64;
        for &t in tokens {
            if t >> 31 == 0 {
                bits += self.literal_lengths[t as usize] as u64;
            } else {
                let (li, di) = (
                    length_index(token_length(t)),
                    distance_index(token_distance(t)),
                );
                bits += (self.literal_lengths[257 + li] + LENGTH_EXTRA[li]) as u64;
                bits += (self.distance_lengths[di] + DIST_EXTRA[di]) as u64;
            }
        }
        bits
    }
    fn write_data(&self, w: &mut BitWriter, tokens: &[Token]) {
        for &t in tokens {
            if t >> 31 == 0 {
                let s = t as usize;
                w.write(self.literal_codes[s], self.literal_lengths[s] as u32);
                continue;
            }
            let (length, distance) = (token_length(t), token_distance(t));
            let li = length_index(length);
            w.write(
                self.literal_codes[257 + li],
                self.literal_lengths[257 + li] as u32,
            );
            w.write(
                (length - LENGTH_BASE[li] as usize) as u32,
                LENGTH_EXTRA[li] as u32,
            );
            let di = distance_index(distance);
            w.write(self.distance_codes[di], self.distance_lengths[di] as u32);
            w.write(
                (distance - DIST_BASE[di] as usize) as u32,
                DIST_EXTRA[di] as u32,
            );
        }
        w.write(
            self.literal_codes[END_OF_BLOCK],
            self.literal_lengths[END_OF_BLOCK] as u32,
        );
    }
}

fn token_length(t: Token) -> usize {
    ((t >> 16) & 0x1ff) as usize
}
fn token_distance(t: Token) -> usize {
    (t & 0xffff) as usize
}

/// A dynamic block's header: the code lengths, themselves Huffman coded.
struct DynamicHeader {
    hlit: usize,
    hdist: usize,
    hclen: usize,
    cl_lengths: Vec<u8>,
    cl_codes: Vec<u32>,
    runs: Vec<(usize, u32)>,
}

impl DynamicHeader {
    fn new(codes: &Codes) -> Self {
        let hlit = 257.max(
            286 - codes.literal_lengths[..286]
                .iter()
                .rev()
                .take_while(|&&l| l == 0)
                .count(),
        );
        let hdist = 1.max(
            30 - codes
                .distance_lengths
                .iter()
                .rev()
                .take_while(|&&l| l == 0)
                .count(),
        );
        let mut all = codes.literal_lengths[..hlit].to_vec();
        all.extend_from_slice(&codes.distance_lengths[..hdist]);
        let runs = run_length_encode(&all);
        let mut freqs = [0u32; 19];
        for &(s, _) in &runs {
            freqs[s] += 1;
        }
        let cl_lengths = huffman_lengths(&freqs, 7);
        let hclen = 4.max(
            19 - CL_ORDER
                .iter()
                .rev()
                .take_while(|&&i| cl_lengths[i] == 0)
                .count(),
        );
        Self {
            hlit,
            hdist,
            hclen,
            cl_codes: canonical_codes(&cl_lengths),
            cl_lengths,
            runs,
        }
    }
    fn bits(&self) -> u64 {
        let mut bits = 14 + 3 * self.hclen as u64;
        for &(s, _) in &self.runs {
            bits += self.cl_lengths[s] as u64 + extra_bits(s) as u64;
        }
        bits
    }
    fn write(&self, w: &mut BitWriter) {
        w.write((self.hlit - 257) as u32, 5);
        w.write((self.hdist - 1) as u32, 5);
        w.write((self.hclen - 4) as u32, 4);
        for &i in &CL_ORDER[..self.hclen] {
            w.write(self.cl_lengths[i] as u32, 3);
        }
        for &(s, extra) in &self.runs {
            w.write(self.cl_codes[s], self.cl_lengths[s] as u32);
            w.write(extra, extra_bits(s));
        }
    }
}

fn write_stored(w: &mut BitWriter, bytes: &[u8], last: bool) {
    let mut chunks = bytes.chunks(65535).peekable();
    if bytes.is_empty() {
        w.write(last as u32, 1);
        w.write(0, 2);
        w.align();
        w.write(0, 16);
        w.write(0xffff, 16);
        return;
    }
    while let Some(chunk) = chunks.next() {
        w.write((last && chunks.peek().is_none()) as u32, 1);
        w.write(0, 2);
        w.align();
        w.write(chunk.len() as u32, 16);
        w.write(!(chunk.len() as u32) & 0xffff, 16);
        w.out.extend_from_slice(chunk);
    }
}

/// Compresses `data` as a zlib stream. The output depends only on `data`.
pub(crate) fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter {
        out: vec![0x78, 0xda],
        bits: 0,
        count: 0,
    };
    let tokens = lz77(data);
    let fixed = Codes::new(fixed_literal_lengths().to_vec(), vec![5; 30]);
    let blocks: Vec<&[Token]> = if tokens.is_empty() {
        vec![&[]]
    } else {
        tokens.chunks(TOKENS_PER_BLOCK).collect()
    };
    let mut start = 0;
    for (b, block) in blocks.iter().enumerate() {
        let last = b + 1 == blocks.len();
        let bytes: usize = block
            .iter()
            .map(|&t| if t >> 31 == 0 { 1 } else { token_length(t) })
            .sum();
        let raw = &data[start..start + bytes];
        start += bytes;

        let mut literal_freqs = [0u32; 286];
        let mut distance_freqs = [0u32; 30];
        literal_freqs[END_OF_BLOCK] = 1;
        for &t in block.iter() {
            if t >> 31 == 0 {
                literal_freqs[t as usize] += 1;
            } else {
                literal_freqs[257 + length_index(token_length(t))] += 1;
                distance_freqs[distance_index(token_distance(t))] += 1;
            }
        }
        let mut literal_lengths = huffman_lengths(&literal_freqs, 15);
        literal_lengths.extend([0, 0]);
        let dynamic = Codes::new(literal_lengths, huffman_lengths(&distance_freqs, 15));
        let header = DynamicHeader::new(&dynamic);

        let dynamic_bits = 3 + header.bits() + dynamic.data_bits(block);
        let fixed_bits = 3 + fixed.data_bits(block);
        let stored_bits = (raw.len().div_ceil(65535).max(1) * (3 + 7 + 32) + raw.len() * 8) as u64;
        if stored_bits < dynamic_bits.min(fixed_bits) {
            write_stored(&mut w, raw, last);
        } else if fixed_bits <= dynamic_bits {
            w.write(last as u32, 1);
            w.write(1, 2);
            fixed.write_data(&mut w, block);
        } else {
            w.write(last as u32, 1);
            w.write(2, 2);
            header.write(&mut w);
            dynamic.write_data(&mut w, block);
        }
    }
    w.align();
    w.out.extend_from_slice(&adler32(data).to_be_bytes());
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(data: &[u8]) -> Vec<u8> {
        let compressed = zlib_compress(data);
        let out = zlib_decompress(&compressed, data.len()).unwrap();
        assert_eq!(out, data);
        compressed
    }

    /// A small deterministic generator for test data.
    fn noise(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.max(1);
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn round_trips() {
        round_trip(b"");
        round_trip(b"a");
        round_trip(b"abcabcabcabcabcabcabc");
        let repetitive: Vec<u8> = (0..200_000)
            .map(|i| (i % 7 * 31 + i / 1000) as u8)
            .collect();
        let compressed = round_trip(&repetitive);
        assert!(compressed.len() < repetitive.len() / 20);
        // Incompressible data falls back to stored blocks.
        let random = noise(150_000, 7);
        let compressed = round_trip(&random);
        assert!(compressed.len() < random.len() + random.len() / 1000 + 64);
        // Skewed data, whose Huffman codes are longer than 15 bits before limiting.
        let mut skewed = Vec::new();
        for s in 0..40u32 {
            let count = 1usize << (s / 2).min(20);
            skewed.extend(std::iter::repeat_n(s as u8 * 3, count.min(50_000)));
        }
        let mixed: Vec<u8> = skewed
            .iter()
            .zip(noise(skewed.len(), 3))
            .map(|(&a, b)| if b < 8 { b } else { a })
            .collect();
        round_trip(&mixed);
        // Long zero runs.
        round_trip(&vec![0; 100_000]);
    }

    #[test]
    fn deterministic() {
        let data = noise(10_000, 9).iter().map(|b| b & 7).collect::<Vec<u8>>();
        assert_eq!(zlib_compress(&data), zlib_compress(&data));
    }

    #[test]
    fn limited_lengths_are_complete() {
        let freqs: Vec<u32> = (0..30).map(|i| 1u32 << i.min(28)).collect();
        for max in [7u8, 9, 15] {
            let lengths = huffman_lengths(&freqs, max);
            assert!(lengths.iter().all(|&l| l >= 1 && l <= max));
            let kraft: u64 = lengths.iter().map(|&l| 1u64 << (max - l)).sum();
            assert_eq!(kraft, 1 << max);
        }
        assert_eq!(huffman_lengths(&[0, 0, 5], 7), vec![1, 0, 1]);
        assert_eq!(huffman_lengths(&[0, 0, 0], 7), vec![1, 1, 0]);
    }

    #[test]
    fn decodes_streams_from_zlib() {
        // Python's zlib.compress(b"hello hello hello hello", 9).
        let stream = [
            0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01, 0x68, 0x03,
            0x08, 0xb1,
        ];
        assert_eq!(
            zlib_decompress(&stream, 100).unwrap(),
            b"hello hello hello hello"
        );
        // Python's zlib.compress(b"spritebox", 0): a stored block.
        let stored = [
            0x78, 0x01, 0x01, 0x09, 0x00, 0xf6, 0xff, 0x73, 0x70, 0x72, 0x69, 0x74, 0x65, 0x62,
            0x6f, 0x78, 0x13, 0x7c, 0x03, 0xe1,
        ];
        assert_eq!(zlib_decompress(&stored, 100).unwrap(), b"spritebox");
    }

    #[test]
    fn rejects_malformed_streams() {
        let good = zlib_compress(b"some text, some text, some more text");
        assert!(zlib_decompress(&good, 10).is_err(), "output limit");
        for cut in 0..good.len() {
            assert!(zlib_decompress(&good[..cut], 100).is_err(), "cut at {cut}");
        }
        let mut bad = good.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert_eq!(
            zlib_decompress(&bad, 100).unwrap_err().code,
            ErrorCode::DecodeError
        );
        // Every corruption is an error or a different output, never a panic.
        for i in 2..good.len() {
            for bit in 0..8 {
                let mut c = good.clone();
                c[i] ^= 1 << bit;
                let _ = zlib_decompress(&c, 1000);
            }
        }
        assert!(
            zlib_decompress(&[0x78, 0xda, 0x07], 100).is_err(),
            "reserved block type"
        );
    }
}
