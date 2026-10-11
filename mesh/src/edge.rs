//! Mesh edge: a line segment connecting two vertices.
//!
//! Matches QGIS `QgsMeshEdge` which represents an edge in a mesh
//! defined by two vertex indices.

/// A single edge in a mesh.
///
/// Matches QGIS `QgsMeshEdge`. Stores indices into the vertex array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshEdge {
    /// Index of the first vertex.
    pub v1: usize,
    /// Index of the second vertex.
    pub v2: usize,
}

impl MeshEdge {
    /// Creates a new mesh edge from two vertex indices.
    pub fn new(v1: usize, v2: usize) -> Self {
        Self { v1, v2 }
    }

    /// Returns the first vertex index.
    pub fn v1(&self) -> usize {
        self.v1
    }

    /// Returns the second vertex index.
    pub fn v2(&self) -> usize {
        self.v2
    }

    /// Returns true if this edge connects the same pair of vertices
    /// as `other` (in either direction).
    pub fn is_same_edge(&self, other: &MeshEdge) -> bool {
        (self.v1 == other.v1 && self.v2 == other.v2)
            || (self.v1 == other.v2 && self.v2 == other.v1)
    }

    /// Returns true if this edge contains the given vertex index.
    pub fn contains_vertex(&self, index: usize) -> bool {
        self.v1 == index || self.v2 == index
    }

    /// Returns the other vertex index given one vertex index.
    /// Returns `None` if `index` is not part of this edge.
    pub fn other_vertex(&self, index: usize) -> Option<usize> {
        if self.v1 == index {
            Some(self.v2)
        } else if self.v2 == index {
            Some(self.v1)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_edge_new() {
        let e = MeshEdge::new(0, 1);
        assert_eq!(e.v1(), 0);
        assert_eq!(e.v2(), 1);
    }

    #[test]
    fn mesh_edge_is_same_edge() {
        let e1 = MeshEdge::new(0, 1);
        let e2 = MeshEdge::new(0, 1);
        let e3 = MeshEdge::new(1, 0);
        let e4 = MeshEdge::new(0, 2);
        assert!(e1.is_same_edge(&e2));
        assert!(e1.is_same_edge(&e3));
        assert!(!e1.is_same_edge(&e4));
    }

    #[test]
    fn mesh_edge_contains_vertex() {
        let e = MeshEdge::new(2, 5);
        assert!(e.contains_vertex(2));
        assert!(e.contains_vertex(5));
        assert!(!e.contains_vertex(0));
        assert!(!e.contains_vertex(3));
    }

    #[test]
    fn mesh_edge_other_vertex() {
        let e = MeshEdge::new(2, 5);
        assert_eq!(e.other_vertex(2), Some(5));
        assert_eq!(e.other_vertex(5), Some(2));
        assert_eq!(e.other_vertex(0), None);
    }
}
