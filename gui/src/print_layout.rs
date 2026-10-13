//! Print layouts, atlas generation, and mobile-friendly rendering.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub width_mm: f64,
    pub height_mm: f64,
    pub orientation_landscape: bool,
}

impl Default for Page {
    fn default() -> Self {
        Self { width_mm: 210.0, height_mm: 297.0, orientation_landscape: false }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrintLayout {
    pub name: String,
    pub pages: Vec<Page>,
    pub items: BTreeMap<String, String>,
    pub atlas_enabled: bool,
    pub atlas_coverage_layer: Option<String>,
}

impl PrintLayout {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pages: vec![Page::default()],
            items: BTreeMap::new(),
            atlas_enabled: false,
            atlas_coverage_layer: None,
        }
    }
    pub fn add_item(&mut self, id: impl Into<String>, kind: impl Into<String>) {
        self.items.insert(id.into(), kind.into());
    }
    pub fn enable_atlas(&mut self, layer: impl Into<String>) {
        self.atlas_enabled = true;
        self.atlas_coverage_layer = Some(layer.into());
    }
    pub fn page_count(&self, atlas_features: usize) -> usize {
        if self.atlas_enabled && atlas_features > 0 {
            atlas_features
        } else {
            self.pages.len().max(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_layout() {
        let l = PrintLayout::new("Report");
        assert_eq!(l.name, "Report");
        assert_eq!(l.page_count(0), 1);
    }
    #[test]
    fn atlas_pages() {
        let mut l = PrintLayout::new("Atlas");
        l.enable_atlas("regions");
        assert_eq!(l.page_count(5), 5);
        assert_eq!(l.atlas_coverage_layer.as_deref(), Some("regions"));
    }
    #[test]
    fn items() {
        let mut l = PrintLayout::new("x");
        l.add_item("map1", "map");
        assert_eq!(l.items.get("map1").map(String::as_str), Some("map"));
    }
}
