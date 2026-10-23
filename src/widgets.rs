//! Named widget states and fixed-size icon collections.
use crate::Document;

/// A control's states, each referring to an ordinary named sprite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSet {
    pub(crate) name: String,
    pub(crate) states: Vec<(String, String)>,
}
impl StateSet {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn states(&self) -> &[(String, String)] {
        &self.states
    }
    pub fn state(&self, name: &str) -> Option<&str> {
        self.states
            .iter()
            .find(|(s, _)| s == name)
            .map(|(_, s)| s.as_str())
    }
}

/// Icons with a shared size, in authored order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IconSet {
    pub(crate) name: String,
    pub(crate) members: Vec<String>,
}
impl IconSet {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn members(&self) -> &[String] {
        &self.members
    }
}

impl Document {
    pub fn states(&self) -> &[StateSet] {
        &self.states
    }
    pub fn state_set(&self, name: &str) -> Option<&StateSet> {
        self.states.iter().find(|s| s.name == name)
    }
    pub fn icon_sets(&self) -> &[IconSet] {
        &self.icon_sets
    }
    pub fn icon_set(&self, name: &str) -> Option<&IconSet> {
        self.icon_sets.iter().find(|s| s.name == name)
    }
}
