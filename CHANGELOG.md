# Changelog

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
