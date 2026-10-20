//! Preview sheets: every definition of a document in one labelled image.

use crate::label;
use crate::{Color, Document, Image, Limits, Result};

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
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            scale: 4,
            grid: true,
            max_width: 1024,
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
}

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
    if !document.sprites.is_empty() {
        sections.push(Section {
            title: "Sprites".into(),
            groups: document
                .sprites
                .iter()
                .map(|s| Group {
                    label: s.name.clone(),
                    items: vec![Item {
                        image: s.render(),
                        caption: Some(format!("{}x{}", s.width, s.height)),
                    }],
                })
                .collect(),
        });
    }
    if sections.is_empty() {
        sections.push(Section {
            title: "Nothing to show".into(),
            groups: Vec::new(),
        });
    }
    draw_sections(&sections, options, limits)
}

struct Placed {
    x: u32,
    y: u32,
}

fn item_size(item: &Item, scale: u32) -> (u32, u32) {
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

fn group_size(group: &Group, scale: u32) -> (u32, u32) {
    let mut width = 0;
    let mut height = 0;
    for (i, item) in group.items.iter().enumerate() {
        let (w, h) = item_size(item, scale);
        width += w + if i > 0 { ITEM_GAP } else { 0 };
        height = height.max(h);
    }
    (
        width.max(label::width(&group.label, TEXT_SCALE)),
        TEXT_HEIGHT + LABEL_GAP + height,
    )
}

fn draw_sections(sections: &[Section], options: &PreviewOptions, limits: &Limits) -> Result<Image> {
    let scale = options.scale;
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
            let (w, h) = group_size(group, scale);
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
            draw_group(&mut image, group, at, options);
        }
    }
    Ok(image)
}

fn draw_group(image: &mut Image, group: &Group, at: &Placed, options: &PreviewOptions) {
    label::draw(
        image,
        &group.label,
        at.x as i64,
        at.y as i64,
        TEXT_SCALE,
        TEXT,
    );
    let mut x = at.x as i64;
    let y = (at.y + TEXT_HEIGHT + LABEL_GAP) as i64;
    for item in &group.items {
        let (w, _) = item_size(item, options.scale);
        draw_item(image, item, x, y, options);
        x += (w + ITEM_GAP) as i64;
    }
}

fn draw_item(image: &mut Image, item: &Item, x: i64, y: i64, options: &PreviewOptions) {
    let s = options.scale;
    let (iw, ih) = (item.image.width(), item.image.height());
    image.fill_rect(x, y, iw * s + 2, ih * s + 2, FRAME);
    let grid = options.grid && s >= 4;
    for py in 0..ih {
        for px in 0..iw {
            let color = item.image.pixel(px, py).unwrap_or_default();
            let left = x + 1 + (px * s) as i64;
            let top = y + 1 + (py * s) as i64;
            image.fill_rect(left, top, s, s, CHECKER[((px + py) % 2) as usize]);
            image.fill_rect(left, top, s, s, color);
            if grid {
                image.fill_rect(left, top, s, 1, GRID);
                image.fill_rect(left, top + 1, 1, s - 1, GRID);
            }
        }
    }
    if let Some(caption) = &item.caption {
        let cy = y + (ih * s + 2 + LABEL_GAP) as i64;
        label::draw(image, caption, x, cy, TEXT_SCALE, CAPTION);
    }
}
