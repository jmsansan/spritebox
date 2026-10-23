// Loads spritebox's exported TTF with ab_glyph (as Diamon's FontLibrary does) and
// checks every glyph at 1x, 2x and 3x its pixel height: ids, advances, metrics, and
// rasterized coverage equal to the glyph's pixels.
use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _};

fn main() {
    let kern_text=include_str!("../../../tests/fixtures/ui-text.spritebox");
    let kern_doc=spritebox::Document::parse(kern_text).unwrap();
    let kern_font=kern_doc.font("hud").unwrap();
    let kern_bytes=kern_font.to_ttf(&Default::default()).unwrap();
    let kern_ab=FontRef::try_from_slice(&kern_bytes).unwrap();
    for k in 1..=3 {
        let scaled=kern_ab.as_scaled(PxScale::from((kern_font.height()*k) as f32));
        assert_eq!(scaled.kern(kern_ab.glyph_id('A'),kern_ab.glyph_id('V')),-(k as f32));
    }
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/font.spritebox")).unwrap();
    let doc = spritebox::Document::parse(&text).unwrap();
    let font = doc.font("hud").unwrap();
    let bytes = font.to_ttf(&spritebox::TtfOptions::default()).unwrap();
    let ab = FontRef::try_from_slice(&bytes).expect("ab_glyph reads the font");
    let (h, b) = (font.height() as f32, font.baseline() as f32);
    let mut checked = 0;
    for k in 1..=3 {
        let scale = PxScale::from(h * k as f32);
        let scaled = ab.as_scaled(scale);
        assert_eq!(scaled.ascent(), b * k as f32);
        assert_eq!(scaled.descent(), -(h - b) * k as f32);
        assert_eq!(scaled.line_gap(), (font.line_height() as f32 - h) * k as f32);
        for glyph in font.glyphs() {
            let id = ab.glyph_id(glyph.char());
            assert_ne!(id.0, 0, "{:?}", glyph.char());
            assert_eq!(scaled.h_advance(id), (glyph.advance() * k) as f32, "{:?}", glyph.char());
            let g = id.with_scale_and_position(scale, ab_glyph::point(0.0, b * k as f32));
            let Some(outlined) = ab.outline_glyph(g) else {
                assert!((0..font.height()).all(|y| (0..glyph.width()).all(|x| !glyph.pixel(x, y))));
                checked += 1;
                continue;
            };
            let bounds = outlined.px_bounds();
            let mut ink = vec![0f32; (glyph.width() * k * font.height() * k) as usize];
            let w = (glyph.width() * k) as i32;
            outlined.draw(|x, y, c| {
                let (px, py) = (bounds.min.x as i32 + x as i32, bounds.min.y as i32 + y as i32);
                if px >= 0 && py >= 0 && px < w && (py as u32) < font.height() * k {
                    ink[(py * w + px) as usize] = c;
                }
            });
            for py in 0..font.height() * k {
                for px in 0..glyph.width() * k {
                    let c = ink[(py as i32 * w + px as i32) as usize];
                    let want = if glyph.pixel(px / k, py / k) { 1.0 } else { 0.0 };
                    assert!((c - want).abs() < 1e-3, "{:?} at {k}x ({px}, {py}): {c}", glyph.char());
                }
            }
            checked += 1;
        }
    }
    println!("ab_glyph {} glyphs, {checked} checks at 1x, 2x and 3x: every pixel exact", font.glyphs().len());
}
