use super::lex::{Node, Token};
use super::{Header, Parsed, Parser, palette_entry};
use crate::{Color, ErrorCode, Palette, Theme};

pub(super) struct ThemeDef<'a> {
    pub name: Token<'a>,
    pub base: Token<'a>,
    pub colors: Vec<(char, Color, Token<'a>)>,
}
impl<'a> Parser<'a, '_> {
    pub(super) fn theme(&mut self, node: &Node<'a>) -> Parsed<()> {
        let header = Header::new(node, "theme")?;
        if header.args.len() != 2 || header.args[0].text != "of" {
            return Err(header
                .name
                .error(ErrorCode::Syntax, "Expected `theme NAME of PALETTE`"));
        }
        let name = header.name;
        let base = header.args[1];
        header.finish("theme", "no options")?;
        if name.text == "all" {
            return Err(name.error(
                ErrorCode::InvalidArgument,
                "The theme name `all` is reserved for export",
            ));
        }
        let mut colors = Vec::new();
        let mut keys = std::collections::HashSet::new();
        for child in &node.children {
            if colors.len() >= self.limits.max_palette_colors {
                return Err(child.line.first().error(
                    ErrorCode::LimitExceeded,
                    "More than the palette color limit in a theme",
                ));
            }
            let (key, color) = palette_entry(child)?;
            if !keys.insert(key) {
                return Err(child
                    .line
                    .first()
                    .error(ErrorCode::DuplicateName, "Duplicate theme key"));
            }
            colors.push((key, color?, child.line.first()));
        }
        self.themes.push(ThemeDef { name, base, colors });
        Ok(())
    }
    pub(super) fn resolve_themes(&mut self, palettes: &[Palette]) -> Vec<Theme> {
        let mut themes: Vec<Theme> = Vec::new();
        for def in &self.themes {
            if themes.iter().any(|t| t.name == def.name.text) {
                self.errors
                    .push(def.name.error(ErrorCode::DuplicateName, "Duplicate theme"));
                continue;
            }
            let Some(base) = palettes.iter().find(|p| p.name == def.base.text) else {
                self.errors.push(
                    def.base
                        .error(ErrorCode::UnknownName, "Unknown theme base palette"),
                );
                continue;
            };
            let mut valid = true;
            for &(key, _, token) in &def.colors {
                if base.color(key).is_none() {
                    self.errors.push(token.error(
                        ErrorCode::UnknownName,
                        "A theme key is absent from its base palette",
                    ));
                    valid = false;
                }
            }
            if valid {
                themes.push(Theme {
                    name: def.name.text.into(),
                    palette: def.base.text.into(),
                    colors: def.colors.iter().map(|&(k, c, _)| (k, c)).collect(),
                });
            }
        }
        themes
    }
}
