# Changelog

## Unreleased

### Added

- The text format's palettes, with `from=` copies, and sprites as grids of keys.
- Diagnostics with a line, a column and a stable `ErrorCode`; `check` returns every
  error in a file at once.
- `Limits` on text, definitions, palettes, sprite and image sizes, and nesting.
- `Image`: RGBA8 pixels, sRGB-encoded, not premultiplied, top row first.
- `png`: a deterministic encoder that writes indexed PNG for pixel art, and a
  decoder for every standard PNG, with the crate's own DEFLATE, zlib, CRC-32 and
  Adler-32.
- Preview sheets of a document's sprites, labelled in a built-in pixel font.
- The `spritebox` command: `check` and `render`, with JSON diagnostics and stable
  exit statuses.
