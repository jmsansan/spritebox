# Backgrounds: finite scenes, surfaces and parallax

Version 0.5 adds static backgrounds built from native layers. A surface is a
single-palette grid of authored keys, separate from a sprite. A layer can combine
surfaces, sprites (including textures and modifier chains) and tilemaps with
different palettes. Backgrounds and surfaces never become implicit atlas entries.
The existing screen, animation, texture and theme formats keep their behavior.

See `examples/backgrounds.spritebox` for a complete 320x180 scene and
`examples/night-shift-background.spritebox` for a static city companion to the
original `examples/night_shift.rs` animation.

The harbor source is 2,732 bytes with LF line endings. Its base export consumes
585,664 scene work units for a 57,600-pixel viewport and seven native layers.
The static city companion is 929 bytes, with 288,000 base-export scene work units
for a 480x300 viewport and one native layer. These are reproducible input/output
and work-counter sizes, not timing promises. The original Rust demo still owns
its 96-frame rainy-city animation; the companion moves representative sky,
buildings, moon, windows, neon and railway geometry into declarative text.

## Rust API

```rust
use spritebox::{BackgroundRenderOptions, Document, ExportOptions};

let document = Document::parse("\
palette p
  a #234a65
surface sky 8x6 palette=p
  rect fill at=0,0 size=8x6 key=a
background land 8x6
  layer sky 8x6 parallax=0/1,0/1
    place surface sky at=0,0
")?;
let background = document.background("land").unwrap();
let image = background.render(&document, &BackgroundRenderOptions { camera: (-4, 0) })?;
assert_eq!(image.width(), 8);
let export = document.export(&ExportOptions::default())?;
assert_eq!(export.backgrounds[0].layers[0].image.height(), 6);
# Ok::<(), spritebox::Error>(())
```

`Document::surfaces`, `surface`, `backgrounds` and `background` inspect immutable
objects. `Surface::key` returns `None` for EMPTY or out-of-bounds cells;
`Surface::profile(name).heights()` exposes original y boundaries.
`Surface::builder` paints typed `Primitive`, `Gradient` and `ProfileOptions`
operations within explicit limits. `Background::builder` adds named layers,
`SourceRef` placements and weighted scatter groups using the same generator as
text parsing. `finish` returns an immutable object. `Document::with_surface` and
`with_background` insert validated objects into a new document; references must
resolve in that document. A background can also render before insertion.
`render_with`, `export_with`, `export_all_themes_with` and `preview_with` accept
explicit limits. Rendering can fail on storage or work exhaustion.

## Coordinates and layer composition

```text
background harbor 320x180 seed=7
  layer town 128x96 at=-8,84 repeat=x parallax=1/4,0/1 clip=0,0,128,96
    place surface buildings at=0,0
    place sprite house:flip_x at=24,96 anchor=bottom-left
    place tilemap docks at=64,0 crop=0,0,64,48
```

Background dimensions are viewport pixels; layer dimensions are the native
canvas and repeat period. Sizes must be positive. Positions are signed i32,
with +x right and +y down. Native layer dimensions fit signed coordinates.
`at` defaults to `0,0` on layers and is required on ordinary placements.
Names use ASCII letters, digits, `_` and `-`; surface/background namespaces are
distinct. Forward and imported references work. Nested backgrounds/screens do
not belong in a background. Core APIs perform no filesystem I/O.

`crop=x,y,w,h` is a positive in-bounds source rectangle. Crop is applied first,
then the anchor: `top-left` subtracts nothing, `bottom-left` subtracts the cropped
height, and `center` subtracts floor(width/2), floor(height/2). Placements clip to
the native layer. The optional in-bounds layer `clip` is baked after composing
items. All rectangle bounds are half-open.

Items draw in source order onto transparent straight RGBA8 native layers. Then
layers draw in source order over the transparent viewport using `Color::over`.
This two-stage grouping is part of the format: integer alpha rounding makes
directly blending every item onto the viewport observably different.

`repeat=none|x|y|both` defaults to none. `parallax=NX/DX,NY/DY` defaults to
`1/1,1/1`; numerators are signed i32 and denominators positive u32. Fractions
are reduced, and zero becomes 0/1. For output pixel (x,y), layer origin (ox,oy)
and camera (cx,cy), sample:

```text
u = x - ox + floor(cx * NX / DX)
v = y - oy + floor(cy * NY / DY)
```

Repeating axes use `rem_euclid(coordinate, native_size)`; other axes return
transparent outside the native bounds. Camera x=-1 with parallax 1/4 samples
an offset of -1. Transparent margins retain their place in the repeat period.
Camera movement samples the same generated keys; it does not regenerate noise,
scatter, profiles or dithering. There is no zoom, rotation, tick or velocity.

## Painting surfaces

```text
surface buildings 128x96 palette=world seed=0
  rect wall at=8,32 size=40x64 key=b
  line aerial from=24,10 to=24,31 key=k
  polygon roof key=r points=4,32 28,12 52,32
  ellipse moon at=84,8 size=24x24 key=m
```

All operations are named; names are unique within their owning surface.
Operations overwrite a covered key, including a key whose current color is
transparent. They do not blend colors. EMPTY is distinct from every palette key,
and changing alpha cannot reveal an overwritten key.

Rectangles fill half-open boxes. Lines include both endpoints, normalize them
lexicographically, then use the dominant axis with nearest-integer minor offsets:
`floor((2*i*abs(minor)+abs(major))/(2*abs(major)))`. Ties round upward in the
minor direction. Clipping preserves the original phase and visits only the
visible dominant-axis range, even for extreme off-canvas endpoints.

Polygons sample pixel centers with even-odd fill. Vertical edge intervals include
the lower y and exclude the upper y. At least three distinct vertices and nonzero
signed area are required; repeated vertices, intersections and overlapping edges
are errors. Reversing winding preserves coverage. Ellipses sample pixel centers
inside a positive bounding box using exact wide integer arithmetic. None of these
operations adds antialiasing or intermediate RGB values.

## Key gradients

```text
surface sky 320x180 palette=world
  gradient dawn axis=y mode=blend dither=bayer4
    stop 0 n
    stop 72 p
    stop 179 y
```

`axis=x|y`; `mode=bands|blend`; `dither=none|bayer2|bayer4|bayer8`, default none.
Stops use single palette keys at strictly increasing in-bounds pixel positions.
A single stop fills uniformly. Before/after the stop range, use the endpoint key.
Bands chooses the last stop at or before the coordinate and accepts only no
dithering. Blend computes `floor((coordinate-a)*65535/(b-a))` and uses the existing
`Dither::pick(t,x,y,2)` to choose an endpoint key. Exact stops select their own key.
No RGB interpolation, luminance ordering or color distance enters selection.
Dither coordinates are surface-local, so crop/camera do not shift the pattern.

## Profiles and deterministic seeds

```text
surface terrain 320x180 palette=world seed=7
  profile ridge key=m baseline=132 amplitude=44 cell=32 octaves=3 roughness=60 seed=11
```

Profiles reuse existing wrapped `Noise` with width=surface width and height=cell,
sampling `value(x,0)` without stretched normalization:
`height[x] = baseline - floor(amplitude * value(x,0)/65535)`.
Require `0 <= amplitude <= baseline <= surface_height`, cell divisibility and
the existing octave/roughness rules. Fill key at `y >= height[x]`. Heights remain
available after later paint operations. A bottom boundary equal to the surface
height leaves that column empty. Large amplitude gives mountain ridges; lower
amplitude and roughness give hills/dunes. These are parameters of the same value
noise, not separate algorithms.

`scene_seed` hashes each identity component as u64 little-endian UTF-8 byte length
followed by its bytes, then each u64 seed in little-endian order. FNV-1a-64 starts
at `0xcbf29ce484222325`, XORs each byte and multiplies with wrapping arithmetic by
`0x100000001b3`. Apply the SplitMix64 finalizer: xor-shift 30/multiply
`0xbf58476d1ce4e5b9`, xor-shift 27/multiply `0x94d049bb133111eb`, xor-shift 31.
Profile identity is `["profile", surface, operation]`; scatter identity is
`["scatter", background, layer, operation]`. Seeds are `[owner_seed, op_seed]`,
default zero. Names, not paths, positions or definition order, scope randomness.

Pinned seed vectors, hexadecimal:

| Identity | Seeds | Result |
| --- | --- | --- |
| profile / s / ridge | 9, 3 | 4b19d0a4f3e342ed |
| scatter / scene / hills / stars | 11, 4 | ae69762aa276d3ae |
| profile / Ω / ridge | 0, 0 | 1b8b20325fd5a34c |

The last vector documents UTF-8 hashing; authored definition names still use the
ASCII name grammar. Existing texture seeds are unchanged.

## Scatter

```text
background harbor 320x180 seed=7
  layer trees 320x180 repeat=x
    scatter grove count=12 spacing=10 attempts=512 seed=4 baseline=terrain(ridge) mask=terrain(m)
      variant tree weight=3
      variant tree:flip_x weight=1
```

Give exactly one of `count=N` or `density=P/Q`, where 0 <= P <= Q and Q > 0.
Density floors eligible anchors * P/Q, before spacing. Ordinary candidates are
native integer pixels; profile candidates use x and the authored `height[x]`.
Masks compare the final raw surface key, independently of alpha, at the candidate
anchor. For a profile boundary, test mask y clamped to 0..H-1. Mask/profile surface
dimensions must equal native layer dimensions. EMPTY never matches.

Every accepted sprite uses bottom-left anchor. Spacing is minimum Euclidean
anchor distance within that group, using shortest wrapped distances on repeated
axes. Duplicates are forbidden even with spacing=0. Candidate rejection never
changes subsequent random draws. For attempt i and lane j, finalize
`seed + i*0x9e3779b97f4a7c15 + j*0xd1b54a32d192ed03` (wrapping u64), then map to
range N with the high 64 bits of the u128 product `random*N`. Lanes 0/1/2 select
x/y/weighted variant. Weights are positive u32, summed in checked u64; weighted
intervals follow authored order. This finite mapping uses no rejection loop.

Attempts default to max(64,32*target); an explicit value must be positive. Target
zero succeeds without attempts. Failure to reach the exact target is an error
showing target, accepted and attempts; tune count, spacing, attempts or mask.
No partial scatter output is returned. Scatter is static decoration, not physics
or alpha collision detection.

## Export and reconstruction

`ExportOptions.backgrounds` selects a name, camera and `BackgroundMode`:
`None`, `Flat`, `Layers` or `Both` (default). Ordinary assets remain exported.
Global background-definition and local layer ordinals determine names before
selection: `background-0000.png`, `background-0000-layer-0000.png`. Theme files
append `-theme-0000` before `.png`; theme order is the document's order. Names do
not become paths. Full output paths are checked case-insensitively before writes.

Root metadata stays format version 1; documents without exported backgrounds
keep their old shape. The optional `backgrounds` object has `schema:1`,
`units:"pixel"` and ordered `items`. Each item stores `name`, `size:[W,H]`, optional
`flat:{image,camera:[x,y]}` and optional `layers`. A layer stores `name`, `image`,
`size`, `origin`, `repeat` and `parallax:{x:{num,den},y:{num,den}}`. Array order is
draw order. Omitted modes produce no dangling filenames. Themes have the same
background section inside their existing theme entry.

Layer PNGs are native straight RGBA8 after crop, anchoring, item composition and
clip. They are not viewport-padded, trimmed, extruded or camera-shifted. To
reconstruct: clear a viewport to transparent; for each layer, apply the exact
floor/remainder formula above at each viewport pixel, fetch its native pixel or
transparent, and source-over onto the viewport using `Color::over`. Do not reapply
clip/crop. `tests/backgrounds.rs` contains an independent implementation and exact
metadata/PNG/key/profile/scatter fixtures under `tests/fixtures/backgrounds`.

## CLI, preview and lint

```sh
spritebox check examples/backgrounds.spritebox --json
spritebox render examples/backgrounds.spritebox --out out/backgrounds --theme all
spritebox render examples/backgrounds.spritebox --out out/camera --background harbor --camera -64,0 --background-mode both --lint
```

Background selectors/camera/mode apply only to directory `render`, and reject
duplicate flags or unknown names. `--max-side` remains the atlas/tileset limit;
background PNGs use image limits and retain native dimensions regardless of
sprite preview `--scale`. All outputs and the preview validate before first write.

Background thumbnails use divisor=max(1,ceil(W/320),ceil(H/180)), output
ceil(W/divisor) by ceil(H/divisor), and nearest samples at coordinate*divisor.
They are labelled with native size and camera and never enlarged by `--scale`.
Intermediate surfaces do not each get a preview entry. Generator/mask keys count
as palette uses. Sprite-only heuristics do not analyze full generated backgrounds.
`background_clipped` reports a fully clipped placement with source position and
imported filename. Existing file/definition allows, strict/deny and warning limits
apply. Parse, generation and resource failures remain errors.

## Budgets and compatibility

`Limits` retains its existing image/sprite/map/nesting/definition bounds and adds:

| Limit | Default | Accounted resources |
| --- | --- | --- |
| max_scene_commands | 4096 | Operations, layers, placements, vertices, stops, variants |
| max_scene_pixels | 16*1024*1024 | Surface keys, height entries, shared source key buffers, placement records, retained/temporary images |
| max_scene_work | 64*1024*1024 | Raster samples, edge tests, noise octaves, proposals and spacing comparisons |
| max_scatter_attempts | 1,000,000 | Candidate attempts across document generation |

Counters are shared across children for each top-level parse/render/export.
Transforms reserve source/output/temporary buffers before execution and charge a
conservative 16 work units per source/output key cell. Retained source buffers are
deduplicated by shared ownership. Export across themes shares one work/storage
budget. CLI export and preview share remaining budget. Platform byte bounds are
checked before allocating, including 32-bit WASM. Raise limits explicitly for
trusted larger inputs. Struct literals can preserve defaults with `..Default::default()`.

Backgrounds preserve safe Rust, zero production dependencies, MSRV 1.85 and the
in-memory WASM core. Cyclic cloud/rain/water motion, frame timelines, world
streaming, automatic background atlasing, new noise algorithms and full Night
Shift animation migration are outside 0.5.
