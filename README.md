# spritebox

[![MSRV 1.85](https://img.shields.io/badge/rust-1.85%2B-blue.svg)](#install)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

**2D pixel art as plain text, in safe Rust with zero dependencies: palettes, sprites,
animations, autotiled tilesets and tilemaps written as text, validated with exact
diagnostics, and rendered to RGBA images, PNG files, atlases with their metadata,
and preview sheets.**

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
| Sprites: grids of keys with a declared size, in layers | Available |
| Derived sprites: flip, rotate, offset, pad, palette swap, outline | Available |
| Animations: frames with ticks, looping or once | Available |
| Diagnostics with line, column and stable code; all errors at once | Available |
| RGBA8 output, PNG writing and reading | Available |
| Tilesets, with 16- and 47-variant autotiling from 4 or 5 hand-drawn pieces | Available |
| Tilemaps: grids of tile indices from a legend, autotiled, with solid tiles | Available |
| Atlases: packed sprites and frames with padding; metadata as Rust structs and JSON | Available |
| Preview sheets | Available (sprites, animations) |
| Command-line tool: `check`, `render` | Available |
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
use spritebox::{AtlasOptions, Document, PreviewOptions, png};

fn build_art(text: &str) -> spritebox::Result<()> {
    let document: Document = text.parse()?;
    // One texture with every sprite and frame, and where each one is.
    let atlas = document.atlas(&AtlasOptions::default())?;
    png::write_file("atlas.png", &atlas.image)?;
    std::fs::write("spritebox.json", atlas.to_json("atlas.png"))?;
    let coin = atlas.region("coin").unwrap(); // Rect { x, y, width, height } in texels
    // A sheet with everything, scaled up and labelled, to review at a glance.
    let sheet = document.preview(&PreviewOptions::default())?;
    png::write_file("preview.png", &sheet)
}
```

The command-line tool does the same for a file:

```sh
spritebox check examples/showcase.spritebox
spritebox render examples/showcase.spritebox --out out
```

![The preview sheet of examples/showcase.spritebox](https://raw.githubusercontent.com/jmsansan/spritebox/main/docs/showcase-preview.png)

## The text format

A file is a list of definitions. Each starts at the first column with a directive
and a name, and the lines under it are indented with spaces: any number, the same
for every line of the block. Tabs are not allowed in indentation. `;` starts a
comment that runs to the end of the line, and blank lines are ignored. Names use
ASCII letters, digits, `_` and `-`. Files are UTF-8; the extension `.spritebox` is
a convention.

```text
; A sprout in two layers, sprites derived from it, and animations.
palette field
  .  transparent
  k  #1a1c2c
  w  #f4f4f4
  g  #38b764
  G  #a7f070
  b  #5d275d
  p  #b13e53
  P  #ef7d57
  o  #f4b41b
  y  #ffe478

palette dusk from=field   ; field's keys; these replace or add
  p  #3b5dc9
  P  #73eff7

sprite sprout 12x12 palette=field
  layer body
    .....gG.....
    ....gGGg....
    .....gg.....
    ...pppppp...
    ..pPPPpppp..
    .pPPpppppbp.
    .pPppppppbp.
    .ppppppppbp.
    .pppppppbbp.
    ..pppppbbp..
    ...pp..pp...
    ...kk..kk...
  layer face
    ............
    ............
    ............
    ............
    ............
    ...wk..wk...
    ...kk..kk...
    ............
    .....kk.....
    ............
    ............
    ............

sprite sprout_left = sprout:flip_x
sprite sprout_outlined = sprout:pad(1):outline(k)

sprite coin 8x8 palette=field
  ..kkkk..
  .kooyyk.
  kooyywok
  kooyyook
  kooooook
  koooooyk
  .koooyk.
  ..kkkk..

animation sprout_idle loop
  sprout              20
  sprout:offset(0,1)  20

animation sprout_hurt once
  sprout:swap(dusk)   4
  sprout              4
```

The full file, with a spinning coin, is
[examples/showcase.spritebox](examples/showcase.spritebox).

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

**Layers.** Instead of rows, a sprite may have layers, drawn bottom first:

```text
sprite hero 16x16 palette=field
  layer body          ; rows of keys, as above, indented under the layer
    ...
  layer shadow = blob:offset(1,1)   ; or another sprite, through a modifier chain
```

Every layer has the sprite's size; a layer made from another sprite must have the
same size and palette. Layers are composited with the "over" operator on sRGB values,
in integer arithmetic, so translucent colors blend the same on every platform.

### Derived sprites and modifiers

`sprite NAME = CHAIN` defines a sprite from another through a **modifier chain**: a
sprite's name and `:modifier`s, applied left to right, written without spaces. The
same chains name the frames of animations and the layers of sprites.

| Modifier | Result |
| --- | --- |
| `flip_x`, `flip_y` | Mirrored left to right, or top to bottom |
| `rot90`, `rot180`, `rot270` | Rotated clockwise; a quarter turn swaps width and height |
| `offset(dx,dy)` | Pixels moved right and down (negative: left and up) within the same size; pixels moved out are lost |
| `pad(n)` | `n` empty pixels added on every side |
| `swap(PALETTE)` | The same keys colored by another palette, which must have every key the sprite uses |
| `outline(KEY)` | A one-pixel outline of `KEY` on the transparent pixels that share an edge with visible ones, as a new bottom layer; pad first if the shape touches the edges |

```text
sprite coin_night = coin:swap(dusk)
sprite arrow_down = arrow_right:rot90
sprite gem_glow = gem:pad(1):outline(y)
```

Sprites may be derived from derived sprites, in any order in the file, but not in a
circle.

### Animations

`animation NAME [loop|once]`, then one frame per line: a sprite's name or a modifier
chain, and how many **ticks** it shows, 1 to 65535. Diamon steps at 60 ticks a
second, so `6` is a tenth of a second. `loop` (the default) starts again after the
last frame; `once` stays on it.

```text
animation coin_spin loop
  coin              6
  coin_edge         6
  coin_thin         6
  coin_edge:flip_x  6
```

A frame's chain builds a sprite named by the chain, such as `coin_edge:flip_x`;
`Document::sprite` finds it, and `Document::frame_sprites` lists them.

### Tilesets

![The preview sheet of examples/tiles.spritebox](https://raw.githubusercontent.com/jmsansan/spritebox/main/docs/tiles-preview.png)

`tileset NAME WxH [palette=PALETTE] [columns=N]`, then its tiles: plain tiles, and
terrains whose variants spritebox builds.

```text
tileset ground 8x8 palette=ground
  tile brick            ; rows of keys, as for a sprite
    rrrRrrrR
    ...
  tile spikes = spike   ; or a sprite of the tile's size and palette, through a chain
  terrain dirt 47       ; five pieces; spritebox builds the 47 variants
    island
      .kkkkkk.
      ...
    h_strip
      ...
    v_strip
      ...
    inner
      ...
    fill
      ...
  terrain stone 16      ; four pieces; spritebox builds the 16 variants
    island = stone_island
    h_strip
      ...
    v_strip
      ...
    fill
      ...
```

**Tile indices** start at 1, so that 0 can mean an empty cell. Tiles are numbered in
the order written, and a terrain takes 16 or 47 consecutive indices, its variants in
order; variant `v` of terrain `dirt` is named `dirt/v`. The tileset's texture is a
grid `columns` tiles wide (8 by default, fewer for fewer tiles), tile 1 at the top
left, row by row, with no spacing.

### Autotiling

A terrain is drawn as a few whole tiles, its **pieces**, each the tile as it looks
with certain neighbours of the same terrain:

| Piece | The tile when it connects to | It shows |
| --- | --- | --- |
| `island` | nothing | edges on all four sides, outer corners |
| `h_strip` | east and west | edges at the top and the bottom |
| `v_strip` | north and south | edges at the left and the right |
| `inner` | north, east, south and west, but no corner | no edges, an inner corner in each corner (47 only) |
| `fill` | all eight neighbours | no edges |

A `16` terrain needs `island`, `h_strip`, `v_strip` and `fill`; a `47` terrain needs
`inner` too. Pieces may be rows or `PIECE = CHAIN`. The tile size must be even.

Each variant is assembled from **quarters**. A quarter looks at the two edge
neighbours beside it (north or south, and west or east) and the corner between them,
and is copied from the same quarter of one piece:

| Edge neighbour north/south | Edge neighbour west/east | Corner | Quarter from |
| --- | --- | --- | --- |
| no | no | any | `island` |
| no | yes | any | `h_strip` |
| yes | no | any | `v_strip` |
| yes | yes | no | `inner` |
| yes | yes | yes | `fill` |

So the north-west quarter of a tile with land to the north and west but not
north-west comes from `inner`, and shows its inner corner. A `16` terrain ignores
corners and treats them as connected, so it never needs `inner`.

**Variant order.** Neighbours are bits, clockwise from north: N = 1, NE = 2, E = 4,
SE = 8, S = 16, SW = 32, W = 64, NW = 128.

- A `16` terrain's variant is `N + 2·E + 4·S + 8·W`, with each letter 1 when that
  neighbour connects: variant 0 is an island, 15 is surrounded.
- A `47` terrain first clears each corner bit unless both edge bits beside it are
  set, since a corner matters only then. The variant is the position of that mask
  among the 47 such masks in ascending order:

  ```text
  0, 1, 4, 5, 7, 16, 17, 20, 21, 23, 28, 29, 31, 64, 65, 68, 69, 71, 80, 81, 84, 85,
  87, 92, 93, 95, 112, 113, 116, 117, 119, 124, 125, 127, 193, 197, 199, 209, 213,
  215, 221, 223, 241, 245, 247, 253, 255
  ```

  Variant 0 is an island, and 46 (mask 255) is surrounded.

`Scheme::variant` and `Terrain::index` compute this in Rust, and `BLOB_MASKS` is the
list. The tests check all 256 neighbour masks against these rules, for both schemes.

### Tilemaps

`tilemap NAME [tileset=TILESET] [outside=same|empty]`, then a `legend` and a `map`:

```text
tilemap level tileset=ground
  legend
    .  empty
    #  dirt    solid    ; a terrain: autotiled
    b  brick   solid    ; a tile
    5  dirt/5           ; one variant of a terrain, fixed
    ^  spikes
  map
    ..........bbbb....
    ...###............
    ...###...^^^......
    ##################
```

- **Legend.** One line per key: one character, then `empty`, a tile, a terrain or a
  terrain's variant `TERRAIN/N`, then `solid` if those tiles are solid.
- **Map.** Rows of legend keys, all of the same width.
- **Autotiling.** A terrain cell takes the variant for its neighbours of the same
  terrain, fixed variants of that terrain included. Different terrains do not
  connect to each other. With `outside=same`, the default, cells beyond the map's
  edges connect, so ground runs off the screen; `outside=empty` closes the edges.
- **Solid tiles.** The map's solid set holds the indices of every solid legend entry,
  all the variants of a solid terrain included. An index cannot be solid under one
  key and not under another. `tileset=` may be left out when the file has one
  tileset.

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

**Atlases.** `Document::atlas` packs every sprite, and every sprite that animation
frames build, into one texture. Each sprite gets `padding` empty pixels on every
side (1 by default, so neighbors are 2 apart), which prevents bleeding with nearest
sampling; `extrude` fills the padding with copies of edge pixels, for linear
sampling. Sprites with identical pixels share one rectangle. Packing is a skyline,
tallest first, trying several widths and keeping the smallest area; it depends only
on the sprites, so the same file gives the same atlas.

The `Atlas` holds the image, each sprite's `Rect` in texels, and the animations with
each frame's rectangle and ticks. `Atlas::to_json` writes the same as JSON; for
`examples/showcase.spritebox`, in part:

```json
{
  "format": "spritebox",
  "version": 1,
  "atlas": {
    "image": "atlas.png",
    "size": [30, 60],
    "padding": 1,
    "sprites": {
      "coin": [1, 31, 8, 8],
      "coin_edge": [1, 41, 8, 8],
      "coin_edge:flip_x": [1, 51, 8, 8]
    }
  },
  "animations": {
    "coin_spin": {
      "loop": true,
      "frames": [
        {"sprite": "coin", "rect": [1, 31, 8, 8], "ticks": 6},
        {"sprite": "coin_edge", "rect": [1, 41, 8, 8], "ticks": 6},
        {"sprite": "coin_edge:flip_x", "rect": [1, 51, 8, 8], "ticks": 6}
      ]
    }
  }
}
```

- Rectangles are `[x, y, width, height]` in texels from the top-left corner.
- `sprites` lists the named sprites in the order defined, then the frame sprites
  in the order first used. Objects keep that order.
- `version` changes only when a change would break a reader; new members may be
  added without one.

**Export.** `Document::export` gives everything a game needs: the atlas, a texture
for each tileset, and the tilemaps. `Export::files` names them `atlas.png`,
`tileset-NAME.png` and `spritebox.json`, and `Export::to_json` adds `tilesets` and
`tilemaps` to the atlas's JSON:

```json
  "tilesets": {
    "ground": {
      "image": "tileset-ground.png",
      "size": [64, 72],
      "tile_size": [8, 8],
      "columns": 8,
      "tiles": ["dirt/0", "dirt/1", "...", "brick", "spikes"],
      "terrains": {"dirt": {"variants": 47, "first": 1}, "stone": {"variants": 16, "first": 48}}
    }
  },
  "tilemaps": {
    "level": {
      "tileset": "ground",
      "size": [32, 9],
      "tiles": [
        [0, 0, 0, 64, 64, 0, "..."],
        [32, 32, 34, 47, 46, 32, "..."]
      ],
      "solid": [1, 2, 3, "..."]
    }
  }
```

(Shortened; the file writes every tile name, row and index, one string or row a line.)
`tiles` lists the tile names in index order, so `tiles[0]` is tile 1. A map's `tiles`
are its rows from the top, each a list of indices, 0 for an empty cell. `solid` is
ascending.

**For Diamon.** Register `atlas.png` with `TextureStore::register` under a URI, with
nearest sampling. A sprite's `Rect` is the `region` of `diamon.sprite2d`, and an
animation's frames, a rectangle and ticks each, are a clip of
`diamon.sprite_animation2d`. A tilemap is a `diamon.tilemap2d`: its tileset's texture
and `tile_size`, its rows of indices with 0 as empty, and its `solid` indices for
collision.

**Preview sheets.** `Document::preview` draws every sprite; every animation's frames
in a row with their ticks; every tileset's tiles with their indices; and the top-left
64 × 36 tiles of each tilemap, beside a copy with its solid tiles tinted red. Sprites
and tiles are scaled up by an integer factor (4 by default), maps by half of it, on a
checkerboard that shows transparency, with lines between pixels or tiles, labelled
with names, sizes and ticks in a small built-in pixel font.

## Command-line tool

```text
spritebox check <file> [--json]
spritebox render <file> --out <dir> [--scale <n>] [--padding <n>] [--extrude] [--json]
```

- `check` validates a file and prints every diagnostic, as
  `file:line:column: error[code]: message` on standard error, or as JSON.
- `render` validates a file, then writes `atlas.png`, `tileset-NAME.png` for each
  tileset, the metadata `spritebox.json`, and `preview.png` to `<dir>`. Nothing is written when the file
  has errors. `--scale` sets the preview's scale, and `--padding` and `--extrude`
  the atlas's.
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
| `max_layers` | 16 layers in a sprite |
| `max_frames` | 1024 frames in an animation |
| `max_tiles` | 4096 tiles in a tileset, terrain variants included |
| `max_map_side` | 1024 tiles |
| `max_nesting` | 32 levels of palettes copied `from=` others, or of sprites derived from sprites |
| `max_image_side` | 8192 pixels, for any image created or decoded |
| `max_image_pixels` | 16 Mi pixels, for any image, and for all sprites together |

```rust
use spritebox::{Document, Limits};

fn parse_trusted(text: &str) -> spritebox::Result<Document> {
    Document::parse_with(text, &Limits { max_sprite_side: 4096, ..Limits::default() })
}
```

## Limitations

- No pixel fonts or procedural textures yet; see the capability table for what comes
  next.
- Terrains connect only to themselves: there are no transitions between terrains,
  such as grass meeting sand, and no animated tiles.
- An atlas is one texture; when the sprites do not fit within
  `Limits::max_image_side`, building it fails rather than spilling into a second.
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
