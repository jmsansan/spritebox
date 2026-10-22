# Changelog

## 0.3.0

Lint: sixteen checks for the signs of careless pixel art, each with its line and
column, marked on previews and explained in the README's "Pixel-art rules"; palette
ramps made with hue shifting; groups; and the Night Shift example.

### Added

- Lint (#21): `Document::lint` and `spritebox lint` report art that is valid but
  strays from pixel-art practice. A `Lint` holds `Warning` values with a stable
  `LintCode`, the position of the pixel's character in its row, the sprite and its
  pixels, and the count of each code. The first checks are `orphan_pixel` and
  `unused_key`. `lint=allow(CODE,...)` on any definition and `lint allow CODE...` in
  a file allow codes on purpose; `--strict` and `--deny` make warnings fail.
  `Limits::max_warnings` bounds the warnings kept, not the counts.
- Lint checks for line work (#22): `jaggy` (uneven steps along a silhouette),
  `double_outline` (outlines two pixels thick at L corners) and `stray_alpha` (lone
  translucent pixels left from smoothing).
- Lint checks for shading and scale (#23): `pillow_shading`, `banding`, `mixel`
  (sprites drawn at 2×–4× among 1× art) and `too_many_colors`, with their
  thresholds in `LintOptions`. Lightness is OKLab's, in fixed point.
- Palette ramps (#24): `ramp NAME KEYS` names a material's keys, dark to light, and
  `ramp NAME KEYS base=#rrggbb shift=DEGREES` makes their colors around a base,
  with hue shifting, deterministically. `Palette::ramps` and `Ramp` are public.
- Lint checks for palettes (#24): `near_duplicate_color`, `flat_ramp` and
  `low_contrast`, with their thresholds in `LintOptions`.
- Groups (#25): `group NAME SPRITE...` names sprites that belong together.
  `Document::groups` and `Group` are public, and the metadata has `groups`.
- Lint checks for sets (#25): `inconsistent_size`, `inconsistent_outline`,
  `inconsistent_palette` and `inconsistent_scale`, across an animation's frames, a
  group's sprites and a tileset's tiles.
- Lint marks on previews (#26): `render --lint` and `PreviewOptions::lint` frame
  each pixel a warning is about, and each sprite a warning is about as a whole, with
  a legend. `examples/lint.spritebox` breaks every rule once, next to the art
  corrected, and the README's "Pixel-art rules" explains each.
- The command-line tool's JSON diagnostics have a `severity`, `error` or `warning`.
- The Night Shift example (#38): an original rainy-city scene in
  `examples/night_shift/`, whose animated preview opens the README.

### Changed

- `PreviewOptions` has a `lint` field: code that builds it field by field adds
  `..PreviewOptions::default()`.
- The metadata has a `groups` member, after `animations`; readers of version 1 that
  ignore unknown members are unaffected.
- The showcase example's `dusk` palette no longer overrides keys its sprite does not
  use, which lint reported.

## 0.2.0

Procedural textures, the last step of the initial plan, and a fix for Windows.

### Added

- Procedural textures: `texture NAME WxH seed=N` with `noise` (seeded value noise
  that wraps, in octaves), a `ramp` of palette keys and ordered `dither`ing. A
  texture is a sprite, so it serves as a layer, a tile, a terrain's piece or a frame.
  `Noise`, `Dither` and `bayer` are public.

### Fixed

- On Windows, the command-line tool printed imported files' paths with mixed
  separators.

## 0.1.0

First release: the art of a 2D game as plain text, from palettes to TrueType fonts,
with the metadata that Diamon's sprites, animations, tilemaps and panels read.

### Added

- The text format's palettes, with `from=` copies, and sprites as grids of keys.
- Diagnostics with a line, a column and a stable `ErrorCode`; `check` returns every
  error in a file at once.
- `import NAME`: definitions of other files, loaded through a caller's loader
  (`Document::parse_with_loader`, `check_with_loader`); the command-line tool reads
  them from disk. Errors in imported files name the file.
- Builds for `wasm32-unknown-unknown`; nothing on the path from text to output
  touches the file system.
- `Limits` on text, definitions, palettes, sprite and image sizes, and nesting.
- `Image`: RGBA8 pixels, sRGB-encoded, not premultiplied, top row first.
- `png`: a deterministic encoder that writes indexed PNG for pixel art, and a
  decoder for every standard PNG, with the crate's own DEFLATE, zlib, CRC-32 and
  Adler-32.
- Layers: rows of keys, or other sprites through modifier chains, composited in
  order.
- Derived sprites and modifier chains: `flip_x`, `flip_y`, `rot90`, `rot180`,
  `rot270`, `offset(dx,dy)`, `pad(n)`, `swap(palette)` and `outline(key)`, also as
  methods of `Sprite`.
- Animations: frames of sprites or chains with ticks, looping or once. A frame's
  trailing flips are flags of the frame, not flipped pixels.
- Panels: `slice=` marks a sprite as a nine-slice panel; its borders follow flips,
  rotations and padding, and appear in the metadata.
- Tileset textures with the atlas's padding and extrusion.
- Tilesets of plain tiles and autotiled terrains: 16 variants from 4 pieces, or 47
  from 5, assembled by quarters from each tile's neighbours.
- Tilemaps: a legend of keys for tiles, terrains, fixed variants and `empty`, a map
  of keys, autotiling with `outside=same|empty`, and solid tiles.
- `Document::export`: the atlas, a texture for each tileset, the tilemaps, and their
  metadata in one JSON file.
- `covers=ascii,latin1,spanish` on a font: `check` reports the glyphs it lacks. The
  example font covers printable ASCII and Spanish.
- Pixel fonts: glyph grids with an advance, a baseline and a line height; text drawn
  with them; and TrueType export whose outlines are the pixels as squares, merged
  where they share an edge, with the em at the glyph height.
- A metadata reference in the README: every field, with its units and origin.
- Atlases: sprites and frame sprites packed into one texture with padding,
  extruded by default, within a largest side of 2048 by default, identical sprites shared, with each sprite's rectangle and
  each animation's frames as Rust structs and as JSON written by the crate.
- Preview sheets of a document's sprites and animations, labelled in a built-in
  pixel font.
- The `spritebox` command: `check`; `render`, which writes the atlas, the tileset
  textures, the fonts, their metadata and the preview sheet; and `font`, which
  writes one font as TrueType; JSON diagnostics and stable exit statuses.
