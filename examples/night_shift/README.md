# Night Shift — Platform 09

An original pixel-art scene: a rainy city, an elevated train, a ramen stall,
a person with an umbrella, a bicycle and a cat. A coral moon and jade neon signs
light the wet pavement.

All geometry is authored in Rust with rectangles, lines, polygons, ellipses and
5 × 7 pixel lettering. The generator writes palette-key grids as `.spritebox`
text; spritebox parses the palettes, renders the sprites, composites transparent
colors and writes the PNGs. The artwork uses no reference bitmap or imported image.

- Native canvas: **480 × 300**.
- Presentation: **1440 × 900**, with integer 3× scaling.
- Palette: **37 entries**, including transparency for rain and light.
- Animation: **96 frames at 24 fps**, a four-second loop.
- Motion: train, train reflections, rain, puddle ripples, steam and sign light.

## Generate images and frames

From the repository root:

```sh
cargo run --release --example night_shift
```

The generator writes the editable art to `examples/night_shift/night_shift.spritebox`
and the images to `out/night-shift/`:

| File | Contents |
| --- | --- |
| `night-shift.png` | Complete scene at 3×. |
| `night-shift-native.png` | Scene at native resolution. |
| `train.png` | Separate train with transparent background, at 4×. |
| `contact-sheet.png` | Four moments from the loop at one-second intervals. |
| `frames/000.png` through `frames/095.png` | Final frames at 3×. |

The `.spritebox` file contains the city, train, foreground architecture,
transparency and a five-layer composite named `night_shift`. That composite
matches animation frame zero exactly. Motion is calculated by the Rust example;
the text asset represents the initial state rather than defining all 96 frames
as a spritebox animation.

## Generate the GIF

With FFmpeg installed:

```sh
bash examples/night_shift/render.sh
```

This also writes `out/night-shift/night-shift.gif`. FFmpeg only packages frames
already drawn and rendered with spritebox. The README preview is stored in
`docs/night-shift.gif`; after changing the art, regenerate it and copy the new GIF
there to update the preview.

The generator checks that the five-layer scene matches the first frame, that
frame 96 matches frame zero to close the loop seamlessly, and that decoding the
native PNG preserves the exact pixels.
