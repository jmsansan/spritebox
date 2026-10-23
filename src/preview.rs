//! Preview sheets: every definition of a document in one labelled image.

use crate::label;
use crate::{Color, Document, Image, Limits, LintCode, LintOptions, Result, Sprite, Warning};

/// How a preview sheet is drawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewOptions {
    /// Screen pixels for each sprite pixel, 1..=64. Default 4.
    pub scale: u32,
    /// Lines between sprite pixels, drawn when `scale` is at least 4. Default true.
    pub grid: bool,
    /// The width at which rows of items wrap, in pixels. A wider item gets a row of
    /// its own and widens the sheet. Default 1024.
    pub max_width: u32,
    /// Lint the document and mark what it reports on the sprites: a frame around
    /// each pixel a warning is about, and the sprite's own frame for a warning about a
    /// whole sprite, in one color for line work and one for shading and scale; with a
    /// legend at the end. Warnings about palettes and sets are not drawn. Default
    /// false.
    pub lint: bool,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            scale: 4,
            grid: true,
            max_width: 1024,
            lint: false,
        }
    }
}

const BACKGROUND: Color = Color::rgb(0x24, 0x26, 0x2e);
const TITLE: Color = Color::rgb(0xf0, 0xf0, 0xf0);
const TEXT: Color = Color::rgb(0xd8, 0xda, 0xe0);
const CAPTION: Color = Color::rgb(0x9a, 0x9e, 0xaa);
const FRAME: Color = Color::rgb(0x55, 0x58, 0x64);
const CHECKER: [Color; 2] = [Color::rgb(0x6b, 0x6e, 0x78), Color::rgb(0x5c, 0x5f, 0x69)];
const GRID: Color = Color::rgba(0, 0, 0, 56);
/// Lint marks: line work, and shading and scale.
const LINES: Color = Color::rgb(0xff, 0x4d, 0x6d);
const SHADING: Color = Color::rgb(0xff, 0xd2, 0x3f);
const CELL_GRID: Color = Color::rgba(255, 255, 255, 48);

const TEXT_SCALE: u32 = 2;
const TEXT_HEIGHT: u32 = label::HEIGHT * TEXT_SCALE;
const MARGIN: u32 = 12;
const GAP: u32 = 12;
const ITEM_GAP: u32 = 6;
/// Space between a label and what it labels.
const LABEL_GAP: u32 = 4;

/// One image in a group, with an optional caption under it.
pub(crate) struct Item {
    pub image: Image,
    pub caption: Option<String>,
    /// A scale for this item instead of the sheet's.
    pub scale: Option<u32>,
    /// Grid lines every `(width, height)` pixels, such as tiles, instead of every
    /// pixel.
    pub cell: Option<(u32, u32)>,
    /// Lint marks: pixels framed in a color.
    pub marks: Vec<(u32, u32, Color)>,
    /// A lint mark on the whole image: its frame in a color.
    pub flag: Option<Color>,
}

impl Item {
    fn new(image: Image, caption: String) -> Self {
        Self {
            image,
            caption: Some(caption),
            scale: None,
            cell: None,
            marks: Vec::new(),
            flag: None,
        }
    }
}

/// Tiles of a tilemap shown at most, from its top-left corner.
const MAP_SAMPLE: (u32, u32) = (64, 36);

/// Items side by side under a label: a sprite, or an animation's frames.
pub(crate) struct Group {
    pub label: String,
    pub items: Vec<Item>,
}

pub(crate) struct Section {
    pub title: String,
    pub groups: Vec<Group>,
}

/// Draws the preview sheet of `document`.
pub(crate) fn sheet(
    document: &Document,
    options: &PreviewOptions,
    limits: &Limits,
) -> Result<Image> {
    if !(1..=64).contains(&options.scale) {
        return Err(crate::Error::invalid("Preview scale must be 1..=64"));
    }
    let mut sections = Vec::new();
    let lint = options
        .lint
        .then(|| document.lint_with(&LintOptions::default(), limits));
    if !document.sprites.is_empty() {
        sections.push(Section {
            title: "Sprites".into(),
            groups: document
                .sprites
                .iter()
                .map(|s| {
                    let mut item = Item::new(s.render(), format!("{}x{}", s.width, s.height));
                    if let Some(lint) = &lint {
                        mark(&mut item, s, &lint.warnings);
                    }
                    Group {
                        label: s.name.clone(),
                        items: vec![item],
                    }
                })
                .collect(),
        });
    }
    if !document.animations.is_empty() {
        sections.push(Section {
            title: "Animations".into(),
            groups: document
                .animations
                .iter()
                .map(|a| Group {
                    label: format!(
                        "{} {} {}t",
                        a.name,
                        if a.looping { "loop" } else { "once" },
                        a.total_ticks()
                    ),
                    items: a
                        .frames
                        .iter()
                        .map(|f| {
                            // As an engine draws the frame: flipped as it says.
                            let image = document
                                .sprite(&f.sprite)
                                .map(|s| {
                                    let s = if f.flip_x { s.flip_x() } else { s.clone() };
                                    let s = if f.flip_y { s.flip_y() } else { s };
                                    s.render()
                                })
                                .unwrap_or_else(|| Image::blank(1, 1));
                            Item::new(image, format!("{}t", f.ticks))
                        })
                        .collect(),
                })
                .collect(),
        });
    }
    if !document.icon_sets.is_empty() {
        sections.push(Section {
            title: "Icon sets".into(),
            groups: document
                .icon_sets
                .iter()
                .map(|set| Group {
                    label: set.name().into(),
                    items: set
                        .members()
                        .iter()
                        .filter_map(|name| document.sprite(name))
                        .map(|s| Item::new(s.render(), s.name.clone()))
                        .collect(),
                })
                .collect(),
        });
    }
    if !document.screens.is_empty() {
        let pixels: u64 = document
            .screens
            .iter()
            .map(|s| s.width as u64 * s.height as u64 * 3)
            .sum();
        if pixels > limits.max_image_pixels {
            return Err(crate::Error::limit(
                "Screen preview images exceed the aggregate pixel limit",
            ));
        }
        let mut groups = Vec::new();
        for screen in &document.screens {
            let image = screen.render(document, limits)?;
            groups.push(Group {
                label: screen.name.clone(),
                items: (1..=3)
                    .map(|scale| Item {
                        scale: Some(scale),
                        ..Item::new(image.clone(), format!("{scale}x"))
                    })
                    .collect(),
            });
        }
        sections.push(Section {
            title: "Screens".into(),
            groups,
        });
    }
    if !document.tilesets.is_empty() {
        sections.push(Section {
            title: "Tilesets".into(),
            groups: document
                .tilesets
                .iter()
                .map(|t| Group {
                    label: format!("{} {}x{}", t.name, t.tile_width, t.tile_height),
                    items: t
                        .tiles
                        .iter()
                        .enumerate()
                        .map(|(i, tile)| Item::new(tile.render(), (i + 1).to_string()))
                        .collect(),
                })
                .collect(),
        });
    }
    if !document.tilemaps.is_empty() {
        let mut groups = Vec::new();
        for map in &document.tilemaps {
            let Some(tileset) = document.tileset(&map.tileset) else {
                continue;
            };
            let (columns, rows) = (MAP_SAMPLE.0.min(map.width), MAP_SAMPLE.1.min(map.height));
            let image = map.render_part(tileset, columns, rows, limits)?;
            let solid = map.render_solid(tileset, columns, rows, limits)?;
            let fits = image.width() * options.scale.div_ceil(2) + 2 <= options.max_width;
            let scale = if fits { options.scale.div_ceil(2) } else { 1 };
            let sample = if (columns, rows) == (map.width, map.height) {
                String::new()
            } else {
                format!(", first {columns}x{rows}")
            };
            let cell = Some((tileset.tile_width, tileset.tile_height));
            groups.push(Group {
                label: format!("{} {}x{} tiles{sample}", map.name, map.width, map.height),
                items: vec![
                    Item {
                        scale: Some(scale),
                        cell,
                        ..Item::new(image, format!("tileset {}", tileset.name))
                    },
                    Item {
                        scale: Some(scale),
                        cell,
                        ..Item::new(solid, "solid".into())
                    },
                ],
            });
        }
        sections.push(Section {
            title: "Tilemaps".into(),
            groups,
        });
    }
    if !document.fonts.is_empty() {
        let mut groups = Vec::new();
        for font in &document.fonts {
            // Every glyph, 24 to a line.
            let chars: Vec<char> = font
                .glyphs
                .iter()
                .map(|g| g.ch)
                .filter(|&c| c != ' ')
                .collect();
            let lines: Vec<String> = chars.chunks(24).map(|c| c.iter().collect()).collect();
            let text = if lines.is_empty() {
                " ".to_owned()
            } else {
                lines.join("\n")
            };
            let image = font.render_with(&text, Color::WHITE, limits)?;
            groups.push(Group {
                label: font.name.clone(),
                items: vec![Item::new(
                    image,
                    format!(
                        "height {} baseline {} line {}",
                        font.height, font.baseline, font.line_height
                    ),
                )],
            });
        }
        sections.push(Section {
            title: "Fonts".into(),
            groups,
        });
    }
    if sections.is_empty() {
        sections.push(Section {
            title: "Nothing to show".into(),
            groups: Vec::new(),
        });
    }
    if let Some(lint) = &lint {
        sections.push(legend(lint));
    }
    draw_sections(&sections, options, limits)
}

/// The color that marks a lint code on a sprite, if it is drawn.
fn mark_color(code: LintCode) -> Option<Color> {
    use LintCode::*;
    match code {
        OrphanPixel | Jaggy | DoubleOutline | StrayAlpha => Some(LINES),
        PillowShading | Banding | Mixel | TooManyColors => Some(SHADING),
        _ => None,
    }
}

/// Marks the warnings about `sprite` on its item.
fn mark(item: &mut Item, sprite: &Sprite, warnings: &[Warning]) {
    for w in warnings {
        if w.sprite.as_deref() != Some(&sprite.name) {
            continue;
        }
        let Some(color) = mark_color(w.code) else {
            continue;
        };
        if w.pixels.is_empty() {
            item.flag = Some(color);
        }
        item.marks
            .extend(w.pixels.iter().map(|&(x, y)| (x, y, color)));
    }
}

/// The legend of the lint marks, with the number of warnings.
fn legend(lint: &crate::Lint) -> Section {
    let swatch = |color: Color, caption: &str| {
        let mut image = Image::blank(1, 1);
        image.fill_rect(0, 0, 1, 1, color);
        Item {
            scale: Some(TEXT_HEIGHT),
            ..Item::new(image, caption.to_owned())
        }
    };
    let total = lint.total();
    Section {
        title: format!("Lint: {total} warning{}", if total == 1 { "" } else { "s" }),
        groups: vec![Group {
            label: "Marked on the sprites".into(),
            items: vec![
                swatch(LINES, "Line work"),
                swatch(SHADING, "Shading and scale"),
            ],
        }],
    }
}

struct Placed {
    x: u32,
    y: u32,
}

fn item_scale(item: &Item, options: &PreviewOptions) -> u32 {
    item.scale.unwrap_or(options.scale)
}

fn item_size(item: &Item, options: &PreviewOptions) -> (u32, u32) {
    let scale = item_scale(item, options);
    let w = item.image.width() * scale + 2;
    let h = item.image.height() * scale + 2;
    match &item.caption {
        Some(c) => (
            w.max(label::width(c, TEXT_SCALE)),
            h + LABEL_GAP + TEXT_HEIGHT,
        ),
        None => (w, h),
    }
}

/// Places a group's items in rows no wider than `max_width` (but at least one item a
/// row). Returns each item's offset under the label and the group's size.
fn group_layout(
    group: &Group,
    options: &PreviewOptions,
    max_width: u32,
) -> (Vec<(u32, u32)>, u32, u32) {
    let mut offsets = Vec::with_capacity(group.items.len());
    let (mut x, mut y, mut row_height, mut width) = (0u32, 0u32, 0u32, 0u32);
    for item in &group.items {
        let (w, h) = item_size(item, options);
        if x > 0 && x + w > max_width {
            x = 0;
            y += row_height + ITEM_GAP;
            row_height = 0;
        }
        offsets.push((x, y));
        x += w + ITEM_GAP;
        width = width.max(x - ITEM_GAP);
        row_height = row_height.max(h);
    }
    let width = width.max(label::width(&group.label, TEXT_SCALE));
    (offsets, width, TEXT_HEIGHT + LABEL_GAP + y + row_height)
}

fn draw_sections(sections: &[Section], options: &PreviewOptions, limits: &Limits) -> Result<Image> {
    let inner = options.max_width.saturating_sub(2 * MARGIN).max(1);
    // Lay out first, in u64 so that huge documents fail the size check, not overflow.
    let mut placements: Vec<(u64, Vec<Placed>)> = Vec::new();
    let mut y = MARGIN as u64;
    let mut sheet_width = MARGIN as u64 * 2 + label::width("Nothing to show", TEXT_SCALE) as u64;
    for section in sections {
        let title_y = y;
        y += (TEXT_HEIGHT + GAP) as u64;
        let mut x = MARGIN as u64;
        let mut row_height = 0u64;
        let mut placed = Vec::new();
        for group in &section.groups {
            let (_, w, h) = group_layout(group, options, inner);
            let (w, h) = (w as u64, h as u64);
            if x > MARGIN as u64
                && x + w > options.max_width as u64 - MARGIN.min(options.max_width) as u64
            {
                x = MARGIN as u64;
                y += row_height + GAP as u64;
                row_height = 0;
            }
            placed.push(Placed {
                x: x.min(u32::MAX as u64) as u32,
                y: y.min(u32::MAX as u64) as u32,
            });
            x += w + GAP as u64;
            row_height = row_height.max(h);
            sheet_width = sheet_width.max(x - GAP as u64 + MARGIN as u64);
        }
        y += row_height
            + if section.groups.is_empty() {
                0
            } else {
                GAP as u64 * 2
            };
        placements.push((title_y, placed));
    }
    let sheet_height = y + MARGIN as u64 - GAP as u64;
    let (width, height) = (
        sheet_width.min(u32::MAX as u64) as u32,
        sheet_height.min(u32::MAX as u64) as u32,
    );
    Image::check_size(width, height, limits)?;
    let mut image = Image::blank(width, height);
    image.fill_rect(0, 0, width, height, BACKGROUND);
    for (section, (title_y, placed)) in sections.iter().zip(&placements) {
        label::draw(
            &mut image,
            &section.title,
            MARGIN as i64,
            *title_y as i64,
            TEXT_SCALE,
            TITLE,
        );
        for (group, at) in section.groups.iter().zip(placed) {
            draw_group(&mut image, group, at, options, inner);
        }
    }
    Ok(image)
}

fn draw_group(image: &mut Image, group: &Group, at: &Placed, options: &PreviewOptions, inner: u32) {
    label::draw(
        image,
        &group.label,
        at.x as i64,
        at.y as i64,
        TEXT_SCALE,
        TEXT,
    );
    let (offsets, _, _) = group_layout(group, options, inner);
    let top = at.y + TEXT_HEIGHT + LABEL_GAP;
    for (item, (dx, dy)) in group.items.iter().zip(offsets) {
        draw_item(image, item, (at.x + dx) as i64, (top + dy) as i64, options);
    }
}

fn draw_item(image: &mut Image, item: &Item, x: i64, y: i64, options: &PreviewOptions) {
    let s = item_scale(item, options);
    let (iw, ih) = (item.image.width(), item.image.height());
    image.fill_rect(x, y, iw * s + 2, ih * s + 2, FRAME);
    let pixel_grid = item.cell.is_none() && options.grid && s >= 4;
    for py in 0..ih {
        for px in 0..iw {
            let color = item.image.pixel(px, py).unwrap_or_default();
            let left = x + 1 + (px * s) as i64;
            let top = y + 1 + (py * s) as i64;
            image.fill_rect(left, top, s, s, CHECKER[((px + py) % 2) as usize]);
            image.fill_rect(left, top, s, s, color);
            if pixel_grid {
                image.fill_rect(left, top, s, 1, GRID);
                image.fill_rect(left, top + 1, 1, s - 1, GRID);
            }
        }
    }
    if let Some((cw, ch)) = item.cell.filter(|_| options.grid) {
        for cx in (cw..iw).step_by(cw.max(1) as usize) {
            image.fill_rect(x + 1 + (cx * s) as i64, y + 1, 1, ih * s, CELL_GRID);
        }
        for cy in (ch..ih).step_by(ch.max(1) as usize) {
            image.fill_rect(x + 1, y + 1 + (cy * s) as i64, iw * s, 1, CELL_GRID);
        }
    }
    // Lint marks: a frame around each pixel, or around the whole image.
    for &(px, py, color) in &item.marks {
        let left = x + 1 + (px * s) as i64;
        let top = y + 1 + (py * s) as i64;
        if s < 3 {
            image.fill_rect(left, top, s, s, color);
            continue;
        }
        image.fill_rect(left, top, s, 1, color);
        image.fill_rect(left, top + s as i64 - 1, s, 1, color);
        image.fill_rect(left, top, 1, s, color);
        image.fill_rect(left + s as i64 - 1, top, 1, s, color);
    }
    if let Some(color) = item.flag {
        let (w, h) = (iw * s + 2, ih * s + 2);
        image.fill_rect(x, y, w, 1, color);
        image.fill_rect(x, y + h as i64 - 1, w, 1, color);
        image.fill_rect(x, y, 1, h, color);
        image.fill_rect(x + w as i64 - 1, y, 1, h, color);
    }
    if let Some(caption) = &item.caption {
        let cy = y + (ih * s + 2 + LABEL_GAP) as i64;
        label::draw(image, caption, x, cy, TEXT_SCALE, CAPTION);
    }
}
