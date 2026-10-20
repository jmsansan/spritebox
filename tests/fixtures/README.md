The PNG files in `png/` were written by tools other than spritebox, with
[`scripts/make-png-fixtures.py`](../../scripts/make-png-fixtures.py), so that the
decoder is checked against other encoders. Each holds a 13×7 pattern that
`tests/png.rs` computes, so the tests know every pixel.

- `ffmpeg-*.png`: FFmpeg 9.0's PNG encoder from raw input in each of its pixel
  formats: `rgba`, `rgb24`, `gray`, `ya8`, `gray16be`, `ya16be`, `rgb48be`,
  `rgba64be`, `monob` (1-bit) and `pal8` (through `palettegen` and `paletteuse`,
  a 256-entry palette). `ffmpeg-rgba-pred-none.png` uses no filtering; the others
  use Paeth.
- `sips-*.png`: macOS 26 Image I/O (`sips -s format png`) re-encoding two FFmpeg
  files, with `sRGB` and `eXIf` chunks. Image I/O stores premultiplied colors, so
  translucent pixels of `sips-rgba.png` may differ from the pattern by a rounding
  step.
- `zlib-*.png`: written by the script itself with Python's zlib, for what neither
  tool writes: Adam7 interlacing (RGBA, and 2-bit grayscale), `tRNS` color keys for
  grayscale and RGB, a 4-bit palette with `tRNS` alpha, a `gAMA` chunk, image data
  split over two `IDAT` chunks, and every filter type.

The fixtures are synthetic, MIT-licensed test data.
