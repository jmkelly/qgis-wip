//! Minimal GeoJSON (RFC 7946) parser/serializer helpers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    #[serde(rename = "type")]
    pub ty: String,
    pub geometry: Option<Geometry>,
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Geometry {
    Point { coordinates: Vec<f64> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub ty: String,
    pub features: Vec<Feature>,
}

impl FeatureCollection {
    pub fn new() -> Self { Self { ty: "FeatureCollection".into(), features: vec![] } }
    pub fn to_json(&self) -> Result<String, String> { serde_json::to_string(self).map_err(|e| e.to_string()) }
    pub fn from_json(s: &str) -> Result<Self, String> { serde_json::from_str(s).map_err(|e| e.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let mut fc = FeatureCollection::new();
        fc.features.push(Feature { ty: "Feature".into(), geometry: Some(Geometry::Point { coordinates: vec![1.0,2.0] }), properties: BTreeMap::new() });
        let s = fc.to_json().unwrap();
        let back = FeatureCollection::from_json(&s).unwrap();
        assert_eq!(back, fc);
    }
}
