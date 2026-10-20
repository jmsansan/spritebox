# spritebox

[![MSRV 1.85](https://img.shields.io/badge/rust-1.85%2B-blue.svg)](#install)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

**2D pixel art as plain text, in safe Rust with zero dependencies: palettes and
sprites written as text, validated with exact diagnostics, and rendered to RGBA
images, PNG files and preview sheets.**

No C libraries, build scripts or `unsafe`: only the standard library, with its own
PNG encoder and decoder, DEFLATE, CRC-32 and Adler-32. The same text gives the same
pixels and the same PNG bytes on every platform.

spritebox is designed first for AI-assisted development. An assistant cannot draw
with a mouse, but it can write precise text, render it, look at the PNG and correct
the text. Everything here serves that loop: a readable format, diagnostics with a
line, a column and a stable code, deterministic output, and preview sheets that show
a whole file at a glance. People read and edit the same files, and diffs show which
pixels changed. It is a sibling of [audiobox](https://github.com/jmsansan/audiobox-rs),
which does the same for game audio.

## Capabilities

| Capability | Status |
| --- | --- |
| Palettes: one-character keys, `#rrggbb`, `#rrggbbaa`, `transparent`, copies with `from=` | Available |
| Sprites: grids of keys with a declared size | Available |
| Diagnostics with line, column and stable code; all errors at once | Available |
| RGBA8 output, PNG writing and reading | Available |
| Preview sheets | Available (sprites) |
| Command-line tool: `check`, `render` | Available |
| Layers, derived sprites (flip, rotate, offset, palette swap, outline) | Planned |
| Animations with ticks per frame | Planned |
| Atlas packing with metadata as Rust structs and JSON | Planned |
| Tilesets with 16- and 47-variant autotiling, tilemaps with solid tiles | Planned |
| Pixel fonts with TrueType export | Planned |
| Seeded procedural textures: value noise and ordered dithering | Planned |

## Install

```toml
[dependencies]
spritebox = "0.1"
```

Rust 1.85 or newer. The command-line tool installs with
`cargo install spritebox`.

## Example

```rust
use spritebox::{Document, PreviewOptions, png};

fn coin() -> spritebox::Result<()> {
    let document: Document = "
palette dawn
  .  transparent
  k  #1a1c2c
  o  #f4b41b
  y  #ffe478
  w  #ffffff

sprite coin 8x8 palette=dawn
  ..kkkk..
  .kooyyk.
  kooyywok
  kooyyook
  kooooook
  koooooyk
  .koooyk.
  ..kkkk..
".parse()?;
    let coin = document.sprite("coin").unwrap().render(); // RGBA8, 8x8
    png::write_file("coin.png", &coin)?;
    let sheet = document.preview(&PreviewOptions::default())?;
    png::write_file("preview.png", &sheet)
}
```

The command-line tool does the same for a file:

```sh
spritebox check examples/coin.spritebox
spritebox render examples/coin.spritebox --out out
```

![The preview sheet of examples/coin.spritebox](https://raw.githubusercontent.com/jmsansan/spritebox/main/docs/coin-preview.png)

## The text format

A file is a list of definitions. Each starts at the first column with a directive
and a name, and the lines under it are indented with spaces: any number, the same
for every line of the block. Tabs are not allowed in indentation. `;` starts a
comment that runs to the end of the line, and blank lines are ignored. Names use
ASCII letters, digits, `_` and `-`. Files are UTF-8; the extension `.spritebox` is
a convention.

```text
; A coin in two palettes, and a gem.
palette dawn
  .  transparent
  k  #1a1c2c
  o  #f4b41b
  y  #ffe478
  w  #ffffff
  g  #38b764
  G  #a7f070

palette dusk from=dawn   ; dawn's keys; these replace or add
  o  #b86f50
  y  #e4a672

sprite coin 8x8 palette=dawn
  ..kkkk..
  .kooyyk.
  kooyywok
  kooyyook
  kooooook
  koooooyk
  .koooyk.
  ..kkkk..

sprite gem 7x7 palette=dawn
  ..kkk..
  .kGGgk.
  kGwGggk
  kGGgggk
  .kgggk.
  ..kgk..
  ...k...
```

### Palettes

`palette NAME [from=OTHER]`, then one line per color: a key and a color.

- **Keys** are one character: any character but spaces, control characters and
  `; ( ) , =`. Keys are case-sensitive, and need not be ASCII.
- **Colors** are `#rrggbb`, `#rrggbbaa` or `transparent`: sRGB, with alpha that is
  not premultiplied. Hex digits may be either case.
- **`from=OTHER`** starts the palette with the keys and colors of `OTHER`, which
  may be defined anywhere in the file. The palette's own lines replace colors of
  keys it shares and add new keys. Chains of copies are allowed; circles are not.

### Sprites

`sprite NAME WxH [palette=PALETTE]`, then `H` rows of `W` keys each.

- Each row is one word: no spaces between keys. A comment may follow it.
- Every key must be in the palette. `palette=` may be left out when the file
  defines exactly one palette.
- A key's color is drawn as is, so a key whose color is `transparent` gives a
  transparent pixel.

## Output

**RGBA8.** `Sprite::render` returns an `Image`: RGBA8 pixels, sRGB-encoded and not
premultiplied, tightly packed, top row first. That is what GPU texture uploads take,
and exactly what Diamon's `TextureStore::register` takes.

**PNG.** `png::encode` writes an indexed PNG (1, 2, 4 or 8 bits a pixel, with
`tRNS` for colors that are not opaque) when the image has at most 256 colors, which
pixel art almost always does, and 8-bit RGBA otherwise. Both are lossless. The file
holds no time stamp, so the same image gives the same bytes. `png::decode` reads
every standard PNG: all color types and bit depths, interlaced or not, with `tRNS`.
It returns RGBA8, rounding 16-bit samples and ignoring gamma and color-space chunks.
It checks CRCs and rejects unknown critical chunks. Compatibility is tested against
files written by FFmpeg and macOS Image I/O, and FFmpeg reads the crate's files.

**Preview sheets.** `Document::preview` draws every sprite scaled up by an integer
factor (4 by default), on a checkerboard that shows transparency, with lines between
pixels and the sprite's name and size in a small built-in pixel font.

## Command-line tool

```text
spritebox check <file> [--json]
spritebox render <file> --out <dir> [--scale <n>] [--json]
```

- `check` validates a file and prints every diagnostic, as
  `file:line:column: error[code]: message` on standard error, or as JSON.
- `render` validates a file, then writes `preview.png` and `sprites/<name>.png`
  to `<dir>`. Nothing is written when the file has errors.
- `<file>` may be `-` for standard input.

Exit statuses are stable: **0** success, **1** the file has errors, **2** bad usage,
**3** an I/O error. With `--json`, standard output is one JSON object:

```json
{"version":1,"file":"bad.spritebox","ok":false,"diagnostics":[
  {"code":"unknown_name","line":4,"column":4,"message":"Key `x` is not in palette `p`"}
],"written":[]}
```

Each diagnostic has a `code` and a `message`, and `line` and `column` when it
concerns a place in the text. `written` lists the files that `render` wrote.

## Errors

Errors are `Result<T, spritebox::Error>` values with a stable `ErrorCode`, a message,
and for text a `Position`: a 1-based line, and a 1-based column counted in
characters. `Document::parse` returns the first error; `spritebox::check` returns all
of them, ordered by position. Malformed input gives an error, never a panic.

| Code | JSON | Meaning |
| --- | --- | --- |
| `Syntax` | `syntax` | Malformed text: an unknown directive, a missing value, bad indentation |
| `InvalidArgument` | `invalid_argument` | A value out of range, such as a color or a size |
| `UnknownName` | `unknown_name` | A palette, sprite or key that is not defined |
| `DuplicateName` | `duplicate_name` | A name or key defined twice |
| `SizeMismatch` | `size_mismatch` | A grid whose rows or columns do not match its size |
| `Cycle` | `cycle` | Definitions that refer to each other in a circle |
| `LimitExceeded` | `limit_exceeded` | Input or output beyond `Limits` |
| `DecodeError` | `decode_error` | A malformed or truncated PNG file |
| `EncodeError` | `encode_error` | Output that cannot be encoded |
| `UnsupportedFormat` | `unsupported_format` | Binary input that is not a PNG, or a PNG feature not read |
| `Io` | `io` | A file that cannot be read or written |

## Limits

`Limits` bound the work that a file or a PNG can cause. The defaults suit
hand-written files and untrusted input; raise them for trusted input.

| Limit | Default |
| --- | --- |
| `max_text_bytes` | 4 MiB |
| `max_definitions` | 4096 |
| `max_palette_colors` | 256 keys |
| `max_sprite_side` | 1024 pixels |
| `max_nesting` | 32 levels of palettes copied `from=` others |
| `max_image_side` | 8192 pixels, for any image created or decoded |
| `max_image_pixels` | 16 Mi pixels, for any image, and for all sprites together |

```rust
use spritebox::{Document, Limits};

fn parse_trusted(text: &str) -> spritebox::Result<Document> {
    Document::parse_with(text, &Limits { max_sprite_side: 4096, ..Limits::default() })
}
```

## Limitations

- Only palettes and sprites so far; see the capability table for what comes next.
- Color is sRGB throughout, and alpha is composited in sRGB with integer arithmetic.
  There is no color management: gamma, ICC and `sRGB` chunks of PNG input are ignored.
- The PNG encoder writes no interlaced, 16-bit, grayscale or metadata chunks, and the
  decoder does not read APNG animation.
- The preview's label font has uppercase letters, digits and common punctuation;
  lowercase letters are drawn as uppercase.

## Development and releases

CI runs only when a release is published (or on demand from the Actions tab), so run
the checks locally before each pull request:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --release
cargo +1.85.0 test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
SPRITEBOX_FFMPEG_TESTS=1 cargo test --test png   # needs FFmpeg
cargo publish --dry-run
```

`scripts/make-png-fixtures.py` regenerates the PNG fixtures with FFmpeg and, on
macOS, `sips`; see [tests/fixtures/README.md](tests/fixtures/README.md).

To release: update `CHANGELOG.md` and the version in `Cargo.toml`, run the checks,
merge to `main`, run `cargo publish`, then tag `vX.Y.Z` and publish a GitHub release,
which runs CI on Linux, macOS and Windows, the MSRV and FFmpeg interoperability.

## License

MIT
