//! Groups: `group NAME SPRITE...`, sprites that belong together, such as the icons of
//! an inventory, for lint and for the metadata.

use std::collections::HashMap;

use super::lex::{Node, Token};
use super::{Parsed, Parser, SetDef, check_name, duplicate};
use crate::lint::Kind;
use crate::{ErrorCode, Group, Position, Sprite};

pub(crate) struct GroupDef<'a> {
    pub name: Token<'a>,
    pub members: Vec<Token<'a>>,
}

impl<'a> Parser<'a, '_> {
    pub(super) fn group(&mut self, node: &Node<'a>) -> Parsed<()> {
        let tokens = &node.line.tokens;
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error("Expected a name for the group".to_owned()))?;
        check_name(name)?;
        let mut members = Vec::new();
        let mut lint = None;
        for &token in &tokens[2..] {
            match token.text.strip_prefix("lint=") {
                Some(value) if lint.is_none() => {
                    lint = Some(Token {
                        text: value,
                        pos: token.at("lint=".len()),
                    });
                }
                _ if lint.is_some() => {
                    return Err(token.error(
                        ErrorCode::Syntax,
                        format!("Unexpected `{}` after `lint=`", token.text),
                    ));
                }
                _ => {
                    check_name(token)?;
                    if let Some(first) = members.iter().find(|m: &&Token| m.text == token.text) {
                        return Err(token.error(
                            ErrorCode::DuplicateName,
                            format!(
                                "Sprite `{}` is in group `{}` already, at column {}",
                                token.text, name.text, first.pos.column
                            ),
                        ));
                    }
                    members.push(token);
                }
            }
        }
        if members.is_empty() {
            return Err(name.end_error(format!(
                "Group `{}` has no sprites; write them after its name, such as `group items \
                 sword shield`",
                name.text
            )));
        }
        if let Some(child) = node.children.first() {
            return Err(child.line.first().error(
                ErrorCode::Syntax,
                "Unexpected indented line under a group; its sprites follow its name",
            ));
        }
        self.lint_option(Kind::Group, name, lint)?;
        self.groups.push(GroupDef { name, members });
        Ok(())
    }

    /// Resolves the groups: their names are unique, and their sprites are defined.
    pub(super) fn resolve_groups(&mut self, sprites: &[Sprite]) -> Vec<Group> {
        let mut seen: HashMap<&str, Position> = HashMap::new();
        let mut groups = Vec::new();
        for def in &self.groups {
            if let Some(&first) = seen.get(def.name.text) {
                self.errors.push(duplicate(def.name, "Group", first));
                continue;
            }
            seen.insert(def.name.text, def.name.pos);
            let mut complete = true;
            for member in &def.members {
                if sprites.iter().any(|s| s.name == member.text) {
                    continue;
                }
                complete = false;
                // A sprite with errors of its own was reported already.
                if !self.sprites.iter().any(|d| d.name.text == member.text) {
                    self.errors.push(member.error(
                        ErrorCode::UnknownName,
                        format!("Unknown sprite `{}`", member.text),
                    ));
                }
            }
            if complete {
                groups.push(Group {
                    name: def.name.text.to_owned(),
                    members: def.members.iter().map(|m| m.text.to_owned()).collect(),
                });
            }
        }
        groups
    }

    /// Each group's sprites, at their positions, for lint.
    pub(super) fn group_sets(&self) -> Vec<SetDef<'a>> {
        self.groups
            .iter()
            .map(|g| {
                let members = g
                    .members
                    .iter()
                    .map(|m| crate::lint::Member {
                        name: m.text.to_owned(),
                        pos: m.pos,
                        source: Some(m.text.to_owned()),
                    })
                    .collect();
                (Kind::Group, g.name, members)
            })
            .collect()
    }
}
