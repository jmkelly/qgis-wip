//! Octree structure for point cloud spatial indexing.
//!
//! Matches QGIS `QgsPointCloudIndex` / PDAL octree which provides
//! fast spatial queries on point cloud data.

/// Position of a node within its parent octree cell.
///
/// Matches the octree child indexing used by QGIS and PDAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OctreePosition {
    /// Bottom-south-west (0, 0, 0).
    BottomSouthWest = 0,
    /// Bottom-south-east (1, 0, 0).
    BottomSouthEast = 1,
    /// Bottom-north-west (0, 1, 0).
    BottomNorthWest = 2,
    /// Bottom-north-east (1, 1, 0).
    BottomNorthEast = 3,
    /// Top-south-west (0, 0, 1).
    TopSouthWest = 4,
    /// Top-south-east (1, 0, 1).
    TopSouthEast = 5,
    /// Top-north-west (0, 1, 1).
    TopNorthWest = 6,
    /// Top-north-east (1, 1, 1).
    TopNorthEast = 7,
}

impl OctreePosition {
    /// Returns the x component (0 or 1).
    pub fn x(&self) -> u8 {
        (*self as u8) & 1
    }

    /// Returns the y component (0 or 1).
    pub fn y(&self) -> u8 {
        ((*self as u8) >> 1) & 1
    }

    /// Returns the z component (0 or 1).
    pub fn z(&self) -> u8 {
        ((*self as u8) >> 2) & 1
    }

    /// Returns the position from x, y, z components.
    pub fn from_xyz(x: u8, y: u8, z: u8) -> Self {
        match (x & 1, y & 1, z & 1) {
            (0, 0, 0) => OctreePosition::BottomSouthWest,
            (1, 0, 0) => OctreePosition::BottomSouthEast,
            (0, 1, 0) => OctreePosition::BottomNorthWest,
            (1, 1, 0) => OctreePosition::BottomNorthEast,
            (0, 0, 1) => OctreePosition::TopSouthWest,
            (1, 0, 1) => OctreePosition::TopSouthEast,
            (0, 1, 1) => OctreePosition::TopNorthWest,
            (1, 1, 1) => OctreePosition::TopNorthEast,
            _ => unreachable!(),
        }
    }

    /// Returns all 8 positions.
    pub fn all() -> [OctreePosition; 8] {
        [
            OctreePosition::BottomSouthWest,
            OctreePosition::BottomSouthEast,
            OctreePosition::BottomNorthWest,
            OctreePosition::BottomNorthEast,
            OctreePosition::TopSouthWest,
            OctreePosition::TopSouthEast,
            OctreePosition::TopNorthWest,
            OctreePosition::TopNorthEast,
        ]
    }
}

/// A node in the point cloud octree.
///
/// Matches QGIS `QgsPointCloudIndex` octree node. Each node covers
/// a 3D bounding box and either contains points directly (leaf) or
/// has up to 8 children.
#[derive(Debug, Clone, PartialEq)]
pub struct OctreeNode {
    /// Bounding box: (min_x, min_y, min_z, max_x, max_y, max_z).
    pub bounds: (f64, f64, f64, f64, f64, f64),
    /// Depth level in the octree (root = 0).
    pub level: usize,
    /// Maximum depth of the octree.
    pub max_level: usize,
    /// Child nodes (None if this is a leaf or child doesn't exist).
    pub children: Vec<Option<OctreeNode>>,
    /// Number of points in this node (including descendants).
    pub point_count: usize,
    /// True if this node is a leaf (has no children).
    pub is_leaf: bool,
}

impl OctreeNode {
    /// Creates a new octree root node with the given bounds.
    pub fn new(bounds: (f64, f64, f64, f64, f64, f64), max_level: usize) -> Self {
        Self {
            bounds,
            level: 0,
            max_level,
            children: vec![None; 8],
            point_count: 0,
            is_leaf: true,
        }
    }

    /// Returns the bounding box.
    pub fn bounds(&self) -> (f64, f64, f64, f64, f64, f64) {
        self.bounds
    }

    /// Returns the level of this node.
    pub fn level(&self) -> usize {
        self.level
    }

    /// Returns the maximum depth of the octree.
    pub fn max_level(&self) -> usize {
        self.max_level
    }

    /// Returns the number of points in this node.
    pub fn point_count(&self) -> usize {
        self.point_count
    }

    /// Returns true if this node is a leaf.
    pub fn is_leaf(&self) -> bool {
        self.is_leaf
    }

    /// Returns true if this node is the root (level 0).
    pub fn is_root(&self) -> bool {
        self.level == 0
    }

    /// Returns true if this node is at the maximum depth.
    pub fn is_max_depth(&self) -> bool {
        self.level >= self.max_level
    }

    /// Returns the center of this node's bounding box.
    pub fn center(&self) -> (f64, f64, f64) {
        let (min_x, min_y, min_z, max_x, max_y, max_z) = self.bounds;
        (
            (min_x + max_x) / 2.0,
            (min_y + max_y) / 2.0,
            (min_z + max_z) / 2.0,
        )
    }

    /// Returns the size of this node's bounding box.
    pub fn size(&self) -> (f64, f64, f64) {
        let (min_x, min_y, min_z, max_x, max_y, max_z) = self.bounds;
        (max_x - min_x, max_y - min_y, max_z - min_z)
    }

    /// Returns the child position for a point within this node.
    pub fn position_for_point(&self, x: f64, y: f64, z: f64) -> OctreePosition {
        let (cx, cy, cz) = self.center();
        let px = if x >= cx { 1 } else { 0 };
        let py = if y >= cy { 1 } else { 0 };
        let pz = if z >= cz { 1 } else { 0 };
        OctreePosition::from_xyz(px, py, pz)
    }

    /// Returns the bounds for a child at the given position.
    pub fn child_bounds(&self, position: OctreePosition) -> (f64, f64, f64, f64, f64, f64) {
        let (min_x, min_y, min_z, max_x, max_y, max_z) = self.bounds;
        let (cx, cy, cz) = self.center();
        let x_min = if position.x() == 0 { min_x } else { cx };
        let x_max = if position.x() == 0 { cx } else { max_x };
        let y_min = if position.y() == 0 { min_y } else { cy };
        let y_max = if position.y() == 0 { cy } else { max_y };
        let z_min = if position.z() == 0 { min_z } else { cz };
        let z_max = if position.z() == 0 { cz } else { max_z };
        (x_min, y_min, z_min, x_max, y_max, z_max)
    }

    /// Returns true if the point (x, y, z) is within this node's bounds.
    pub fn contains_point(&self, x: f64, y: f64, z: f64) -> bool {
        let (min_x, min_y, min_z, max_x, max_y, max_z) = self.bounds;
        x >= min_x && x <= max_x && y >= min_y && y <= max_y && z >= min_z && z <= max_z
    }

    /// Returns the number of children that exist.
    pub fn child_count(&self) -> usize {
        self.children.iter().filter(|c| c.is_some()).count()
    }

    /// Returns true if this node has a child at the given position.
    pub fn has_child(&self, position: OctreePosition) -> bool {
        self.children[position as usize].is_some()
    }

    /// Returns a reference to the child at the given position.
    pub fn child(&self, position: OctreePosition) -> Option<&OctreeNode> {
        self.children[position as usize].as_ref()
    }

    /// Splits this node into 8 children.
    ///
    /// Does nothing if already at max depth or already split.
    pub fn split(&mut self) {
        if !self.is_leaf || self.is_max_depth() {
            return;
        }
        for pos in OctreePosition::all() {
            let bounds = self.child_bounds(pos);
            self.children[pos as usize] = Some(OctreeNode {
                bounds,
                level: self.level + 1,
                max_level: self.max_level,
                children: vec![None; 8],
                point_count: 0,
                is_leaf: true,
            });
        }
        self.is_leaf = false;
    }

    /// Returns the total number of points in this subtree.
    pub fn total_points(&self) -> usize {
        if self.is_leaf {
            self.point_count
        } else {
            self.children
                .iter()
                .filter_map(|c| c.as_ref())
                .map(|c| c.total_points())
                .sum()
        }
    }

    /// Returns the height of the subtree rooted at this node.
    pub fn height(&self) -> usize {
        if self.is_leaf {
            0
        } else {
            1 + self
                .children
                .iter()
                .filter_map(|c| c.as_ref())
                .map(|c| c.height())
                .max()
                .unwrap_or(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octree_position_xyz() {
        assert_eq!(OctreePosition::BottomSouthWest.x(), 0);
        assert_eq!(OctreePosition::BottomSouthWest.y(), 0);
        assert_eq!(OctreePosition::BottomSouthWest.z(), 0);

        assert_eq!(OctreePosition::TopNorthEast.x(), 1);
        assert_eq!(OctreePosition::TopNorthEast.y(), 1);
        assert_eq!(OctreePosition::TopNorthEast.z(), 1);

        assert_eq!(OctreePosition::BottomNorthWest.x(), 0);
        assert_eq!(OctreePosition::BottomNorthWest.y(), 1);
        assert_eq!(OctreePosition::BottomNorthWest.z(), 0);
    }

    #[test]
    fn octree_position_from_xyz() {
        assert_eq!(OctreePosition::from_xyz(0, 0, 0), OctreePosition::BottomSouthWest);
        assert_eq!(OctreePosition::from_xyz(1, 0, 0), OctreePosition::BottomSouthEast);
        assert_eq!(OctreePosition::from_xyz(0, 1, 0), OctreePosition::BottomNorthWest);
        assert_eq!(OctreePosition::from_xyz(1, 1, 0), OctreePosition::BottomNorthEast);
        assert_eq!(OctreePosition::from_xyz(0, 0, 1), OctreePosition::TopSouthWest);
        assert_eq!(OctreePosition::from_xyz(1, 0, 1), OctreePosition::TopSouthEast);
        assert_eq!(OctreePosition::from_xyz(0, 1, 1), OctreePosition::TopNorthWest);
        assert_eq!(OctreePosition::from_xyz(1, 1, 1), OctreePosition::TopNorthEast);
    }

    #[test]
    fn octree_position_all() {
        let all = OctreePosition::all();
        assert_eq!(all.len(), 8);
    }

    #[test]
    fn octree_node_new() {
        let node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        assert_eq!(node.bounds(), (0.0, 0.0, 0.0, 10.0, 10.0, 10.0));
        assert_eq!(node.level(), 0);
        assert_eq!(node.max_level(), 5);
        assert!(node.is_leaf());
        assert!(node.is_root());
        assert!(!node.is_max_depth());
        assert_eq!(node.point_count(), 0);
    }

    #[test]
    fn octree_node_center() {
        let node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 20.0, 30.0), 5);
        let (cx, cy, cz) = node.center();
        assert_eq!(cx, 5.0);
        assert_eq!(cy, 10.0);
        assert_eq!(cz, 15.0);
    }

    #[test]
    fn octree_node_size() {
        let node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 20.0, 30.0), 5);
        let (sx, sy, sz) = node.size();
        assert_eq!(sx, 10.0);
        assert_eq!(sy, 20.0);
        assert_eq!(sz, 30.0);
    }

    #[test]
    fn octree_node_contains_point() {
        let node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        assert!(node.contains_point(5.0, 5.0, 5.0));
        assert!(node.contains_point(0.0, 0.0, 0.0));
        assert!(node.contains_point(10.0, 10.0, 10.0));
        assert!(!node.contains_point(11.0, 5.0, 5.0));
        assert!(!node.contains_point(5.0, -1.0, 5.0));
    }

    #[test]
    fn octree_node_position_for_point() {
        let node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        // Center is (5, 5, 5)
        assert_eq!(
            node.position_for_point(2.0, 2.0, 2.0),
            OctreePosition::BottomSouthWest
        );
        assert_eq!(
            node.position_for_point(8.0, 2.0, 2.0),
            OctreePosition::BottomSouthEast
        );
        assert_eq!(
            node.position_for_point(2.0, 8.0, 2.0),
            OctreePosition::BottomNorthWest
        );
        assert_eq!(
            node.position_for_point(8.0, 8.0, 8.0),
            OctreePosition::TopNorthEast
        );
    }

    #[test]
    fn octree_node_child_bounds() {
        let node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        let sw = node.child_bounds(OctreePosition::BottomSouthWest);
        assert_eq!(sw, (0.0, 0.0, 0.0, 5.0, 5.0, 5.0));
        let ne = node.child_bounds(OctreePosition::TopNorthEast);
        assert_eq!(ne, (5.0, 5.0, 5.0, 10.0, 10.0, 10.0));
    }

    #[test]
    fn octree_node_split() {
        let mut node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        assert!(node.is_leaf());
        node.split();
        assert!(!node.is_leaf());
        assert_eq!(node.child_count(), 8);
        for pos in OctreePosition::all() {
            assert!(node.has_child(pos));
            let child = node.child(pos).unwrap();
            assert_eq!(child.level(), 1);
            assert!(child.is_leaf());
        }
    }

    #[test]
    fn octree_node_split_at_max_depth() {
        let mut node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 0);
        assert!(node.is_max_depth());
        node.split();
        assert!(node.is_leaf());
        assert_eq!(node.child_count(), 0);
    }

    #[test]
    fn octree_node_height() {
        let mut node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        assert_eq!(node.height(), 0);
        node.split();
        assert_eq!(node.height(), 1);
    }

    #[test]
    fn octree_node_total_points() {
        let mut node = OctreeNode::new((0.0, 0.0, 0.0, 10.0, 10.0, 10.0), 5);
        node.point_count = 10;
        assert_eq!(node.total_points(), 10);
        node.split();
        // After split, point_count is still 10 but children have 0
        // total_points sums children (0) since not a leaf
        assert_eq!(node.total_points(), 0);
    }
}
