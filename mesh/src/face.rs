//! Mesh face: a polygon defined by vertex indices.
//!
//! Matches QGIS `QgsMeshFace` which represents a face in a mesh
//! defined by a list of vertex indices.

/// A single face in a mesh.
///
/// Matches QGIS `QgsMeshFace`. Stores indices into the vertex array.
/// Faces are typically triangles or quads.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeshFace {
    /// Vertex indices defining the face (in order, counter-clockwise).
    pub vertices: Vec<usize>,
}

impl MeshFace {
    /// Creates a new mesh face from vertex indices.
    pub fn new(vertices: Vec<usize>) -> Self {
        Self { vertices }
    }

    /// Creates a triangular face.
    pub fn triangle(v0: usize, v1: usize, v2: usize) -> Self {
        Self {
            vertices: vec![v0, v1, v2],
        }
    }

    /// Creates a quadrilateral face.
    pub fn quad(v0: usize, v1: usize, v2: usize, v3: usize) -> Self {
        Self {
            vertices: vec![v0, v1, v2, v3],
        }
    }

    /// Returns the number of vertices in this face.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Returns true if this face is a triangle (3 vertices).
    pub fn is_triangle(&self) -> bool {
        self.vertices.len() == 3
    }

    /// Returns true if this face is a quad (4 vertices).
    pub fn is_quad(&self) -> bool {
        self.vertices.len() == 4
    }

    /// Returns true if this face has no vertices.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Returns the vertex indices.
    pub fn vertices(&self) -> &[usize] {
        &self.vertices
    }

    /// Returns the edges of this face as pairs of vertex indices.
    pub fn edges(&self) -> Vec<(usize, usize)> {
        let mut edges = Vec::new();
        let n = self.vertices.len();
        if n < 2 {
            return edges;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            edges.push((self.vertices[i], self.vertices[j]));
        }
        edges
    }

    /// Returns true if this face contains the given vertex index.
    pub fn contains_vertex(&self, index: usize) -> bool {
        self.vertices.contains(&index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_face_triangle() {
        let f = MeshFace::triangle(0, 1, 2);
        assert!(f.is_triangle());
        assert!(!f.is_quad());
        assert_eq!(f.vertex_count(), 3);
        assert!(!f.is_empty());
    }

    #[test]
    fn mesh_face_quad() {
        let f = MeshFace::quad(0, 1, 2, 3);
        assert!(!f.is_triangle());
        assert!(f.is_quad());
        assert_eq!(f.vertex_count(), 4);
    }

    #[test]
    fn mesh_face_edges() {
        let f = MeshFace::triangle(0, 1, 2);
        let edges = f.edges();
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[0], (0, 1));
        assert_eq!(edges[1], (1, 2));
        assert_eq!(edges[2], (2, 0));
    }

    #[test]
    fn mesh_face_quad_edges() {
        let f = MeshFace::quad(0, 1, 2, 3);
        let edges = f.edges();
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0], (0, 1));
        assert_eq!(edges[1], (1, 2));
        assert_eq!(edges[2], (2, 3));
        assert_eq!(edges[3], (3, 0));
    }

    #[test]
    fn mesh_face_contains_vertex() {
        let f = MeshFace::triangle(0, 1, 2);
        assert!(f.contains_vertex(0));
        assert!(f.contains_vertex(1));
        assert!(f.contains_vertex(2));
        assert!(!f.contains_vertex(3));
    }

    #[test]
    fn mesh_face_empty() {
        let f = MeshFace::new(vec![]);
        assert!(f.is_empty());
        assert_eq!(f.vertex_count(), 0);
        assert!(f.edges().is_empty());
    }

    #[test]
    fn mesh_face_vertices() {
        let f = MeshFace::new(vec![10, 20, 30]);
        assert_eq!(f.vertices(), &[10, 20, 30]);
    }
}
