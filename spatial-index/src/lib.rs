//! STRtree spatial index for the QGIS Rust port.
//!
//! Pure-Rust replacement for QGIS's `spatialindex` library (libspatialindex),
//! which provides the R-tree backing `QgsSpatialIndex`. Implements:
//!
//! * **Bulk-load** via STR (Sort-Tile-Recursive) packing — O(n log n) build
//!   with no overlaps between sibling nodes at each level.
//! * **Insert** with quadratic split overflow handling.
//! * **Remove** with bbox propagation (lazy condensing).
//! * **Query** by bounding-box intersection.
//! * **Nearest neighbor** via branch-and-bound.
//!
//! The tree is arena-based (flat `Vec` of nodes) to avoid `Box`/`RefCell`
//! overhead and to make node removal and reuse straightforward.
//!
//! Matches QGIS `QgsSpatialIndex` semantics:
//! * `insert` → `QgsSpatialIndex::insertFeature`
//! * `remove` → `QgsSpatialIndex::deleteFeature`
//! * `query_bbox` → `QgsSpatialIndex::intersects`
//! * `nearest_neighbor` → `QgsSpatialIndex::nearestNeighbor`
//!
//! Pure Rust, zero `unsafe`, per ADR-0001.

#![forbid(unsafe_code)]

mod bbox;

pub use bbox::BoundingBox;

use std::cmp::Reverse;
use std::collections::BinaryHeap;

// ---------------------------------------------------------------------------
// Internal node representation
// ---------------------------------------------------------------------------

/// Node identifier (index into the arena).
type NodeId = usize;

/// A single node in the R-tree.
///
/// Leaf nodes hold `Entry::Leaf` items; internal nodes hold `Entry::Internal`
/// children. Each node tracks the union of its children's bboxes for fast
/// intersection tests.
struct Node<T> {
    bbox: BoundingBox,
    entries: Vec<Entry<T>>,
    is_leaf: bool,
}

impl<T> Node<T> {
    fn new(is_leaf: bool) -> Self {
        Self {
            bbox: BoundingBox::default(),
            entries: Vec::new(),
            is_leaf,
        }
    }
}

/// An entry in a tree node — either a leaf item or an internal child.
#[derive(Clone)]
enum Entry<T> {
    /// A leaf item: bounding box + payload.
    Leaf {
        bbox: BoundingBox,
        data: T,
    },
    /// An internal child: bounding box + child node id.
    Internal {
        bbox: BoundingBox,
        child: NodeId,
    },
}

impl<T> Entry<T> {
    /// Returns the bounding box of this entry.
    pub fn bbox(&self) -> BoundingBox {
        match self {
            Entry::Leaf { bbox, .. } => *bbox,
            Entry::Internal { bbox, .. } => *bbox,
        }
    }
}

// ---------------------------------------------------------------------------
// OrderedF64 — f64 wrapper implementing Ord for BinaryHeap
// ---------------------------------------------------------------------------

/// Wrapper around `f64` that implements `Ord` by treating NaN as less than
/// all other values. Used for the nearest-neighbor priority queue.
#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedF64(f64);

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Treat NaN as smallest; otherwise standard comparison.
        match (self.0.is_nan(), other.0.is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => {
                // partial_cmp never returns None when neither is NaN.
                self.0.partial_cmp(&other.0).unwrap()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// STRtree
// ---------------------------------------------------------------------------

/// A pure-Rust STRtree (Sort-Tile-Recursive packed R-tree).
///
/// Replaces QGIS's `spatialindex` library. Supports bulk-load, insert,
/// remove, bbox-intersection query, and nearest-neighbor query.
///
/// # Type parameters
///
/// * `T` — the payload stored alongside each bounding box. Must implement
///   `Clone` for insert/bulk-load and `PartialEq` for remove.
///
/// # Examples
///
/// ```
/// use qgis_spatial_index::{BoundingBox, STRtree};
///
/// let items = vec![
///     (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "a"),
///     (BoundingBox::new(2.0, 2.0, 3.0, 3.0), "b"),
///     (BoundingBox::new(5.0, 5.0, 6.0, 6.0), "c"),
/// ];
///
/// let tree: STRtree<&str> = STRtree::bulk_load(items);
/// assert_eq!(tree.len(), 3);
///
/// let results = tree.query_bbox(&BoundingBox::new(0.5, 0.5, 2.5, 2.5));
/// assert_eq!(results.len(), 2);
/// ```
pub struct STRtree<T> {
    nodes: Vec<Node<T>>,
    free_list: Vec<NodeId>,
    root: Option<NodeId>,
    size: usize,
    node_capacity: usize,
}

impl<T> STRtree<T> {
    /// Creates a new empty tree with the given node capacity.
    ///
    /// `node_capacity` is clamped to a minimum of 2. Typical values are
    /// 8–32; larger values reduce tree height but increase per-node search
    /// cost.
    pub fn new(node_capacity: usize) -> Self {
        Self {
            nodes: Vec::new(),
            free_list: Vec::new(),
            root: None,
            size: 0,
            node_capacity: node_capacity.max(2),
        }
    }

    /// Creates a new empty tree with the default node capacity (10).
    pub fn with_defaults() -> Self {
        Self::new(10)
    }

    /// Returns the number of items in the tree.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns true if the tree contains no items.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns the node capacity (max entries per node).
    pub fn node_capacity(&self) -> usize {
        self.node_capacity
    }

    /// Returns the minimum number of entries per non-root node.
    fn min_entries(&self) -> usize {
        (self.node_capacity + 1) / 2
    }

    // -- Arena management ---------------------------------------------------

    /// Allocates a new node, reusing a freed slot if available.
    fn alloc_node(&mut self, is_leaf: bool) -> NodeId {
        if let Some(id) = self.free_list.pop() {
            self.nodes[id] = Node::new(is_leaf);
            id
        } else {
            let id = self.nodes.len();
            self.nodes.push(Node::new(is_leaf));
            id
        }
    }

    /// Frees a node, returning its slot to the free list.
    fn free_node(&mut self, id: NodeId) {
        self.nodes[id] = Node::new(true);
        self.free_list.push(id);
    }

    // -- Bbox helpers -------------------------------------------------------

    /// Recomputes the bbox of a node as the union of its entries' bboxes.
    fn update_bbox(&mut self, node_id: NodeId) {
        let mut bbox = BoundingBox::default();
        for entry in &self.nodes[node_id].entries {
            bbox = bbox.union(&entry.bbox());
        }
        self.nodes[node_id].bbox = bbox;
    }

    /// Recomputes bboxes for every node on `path` (root..=leaf), bottom-up.
    ///
    /// A parent stores its own copy of each child's bbox in the child's
    /// `Entry::Internal`; `update_bbox` alone does not pick up a child whose
    /// bbox changed, because it unions the *stored* entry bboxes. This walks
    /// from the leaf upward, syncing each parent's entry for its child to the
    /// child's current bbox before recomputing the parent. Without it, a bbox
    /// that grows on insert (or shrinks on remove) leaves stale ancestor
    /// entries, and bbox-pruned searches silently lose items.
    fn refresh_path_entries(&mut self, path: &[NodeId]) {
        for i in (0..path.len()).rev() {
            let node_id = path[i];
            self.update_bbox(node_id);
            if i > 0 {
                let parent_id = path[i - 1];
                let child_bbox = self.nodes[node_id].bbox;
                for entry in &mut self.nodes[parent_id].entries {
                    if let Entry::Internal { bbox, child } = entry {
                        if *child == node_id {
                            *bbox = child_bbox;
                        }
                    }
                }
            }
        }
    }

    // -- Path finding --------------------------------------------------------

    /// Finds the path of node ids from the root to `target`.
    fn find_path_to_node(&self, target: NodeId) -> Vec<NodeId> {
        let root_id = match self.root {
            Some(id) => id,
            None => return Vec::new(),
        };
        let mut path = Vec::new();
        if self.find_path_recursive(root_id, target, &mut path) {
            path
        } else {
            Vec::new()
        }
    }

    fn find_path_recursive(&self, node_id: NodeId, target: NodeId, path: &mut Vec<NodeId>) -> bool {
        path.push(node_id);
        if node_id == target {
            return true;
        }
        let node = &self.nodes[node_id];
        if node.is_leaf {
            path.pop();
            return false;
        }
        for entry in &node.entries {
            if let Entry::Internal { child, .. } = entry {
                if self.find_path_recursive(*child, target, path) {
                    return true;
                }
            }
        }
        path.pop();
        false
    }

    // -- Query ---------------------------------------------------------------

    /// Returns all payloads whose bounding boxes intersect `query`.
    ///
    /// Matches QGIS `QgsSpatialIndex::intersects`.
    pub fn query_bbox(&self, query: &BoundingBox) -> Vec<&T> {
        let mut results = Vec::new();
        if let Some(root_id) = self.root {
            self.query_node(root_id, query, &mut results);
        }
        results
    }

    fn query_node<'a>(&'a self, node_id: NodeId, query: &BoundingBox, results: &mut Vec<&'a T>) {
        let node = &self.nodes[node_id];
        for entry in &node.entries {
            if !entry.bbox().intersects(query) {
                continue;
            }
            match entry {
                Entry::Leaf { data, .. } => results.push(data),
                Entry::Internal { child, .. } => self.query_node(*child, query, results),
            }
        }
    }

    // -- Nearest neighbor ----------------------------------------------------

    /// Returns the payload whose bounding-box center is nearest to `(x, y)`.
    ///
    /// Uses branch-and-bound with a min-heap ordered by minimum possible
    /// distance. The distance metric is the Euclidean distance from the
    /// query point to the nearest point in the item's bounding box.
    ///
    /// Matches QGIS `QgsSpatialIndex::nearestNeighbor`.
    pub fn nearest_neighbor(&self, x: f64, y: f64) -> Option<&T> {
        let root_id = self.root?;

        // Min-heap of (min_possible_distance, node_id).
        let mut heap: BinaryHeap<Reverse<(OrderedF64, NodeId)>> = BinaryHeap::new();
        heap.push(Reverse((OrderedF64(0.0), root_id)));

        let mut best: Option<(f64, &T)> = None;

        while let Some(Reverse((min_dist, node_id))) = heap.pop() {
            // If this node's minimum distance exceeds the best found, stop.
            if let Some((best_dist, _)) = best {
                if min_dist.0 >= best_dist {
                    break;
                }
            }

            let node = &self.nodes[node_id];
            for entry in &node.entries {
                let entry_min_dist = entry.bbox().min_distance_to_point(x, y);
                if let Some((best_dist, _)) = best {
                    if entry_min_dist >= best_dist {
                        continue;
                    }
                }

                match entry {
                    Entry::Leaf { bbox, data } => {
                        // Distance to nearest point in the bbox.
                        let dist = bbox.min_distance_to_point(x, y);
                        if best.map_or(true, |(d, _)| dist < d) {
                            best = Some((dist, data));
                        }
                    }
                    Entry::Internal { child, .. } => {
                        heap.push(Reverse((OrderedF64(entry_min_dist), *child)));
                    }
                }
            }
        }

        best.map(|(_, data)| data)
    }

    // -- Internal: collect entries -------------------------------------------

    /// Collects all leaf entries from a subtree into a flat vec.
    fn collect_entries(&self, node_id: NodeId, out: &mut Vec<(BoundingBox, T)>)
    where
        T: Clone,
    {
        let node = &self.nodes[node_id];
        for entry in &node.entries {
            match entry {
                Entry::Leaf { bbox, data } => out.push((*bbox, data.clone())),
                Entry::Internal { child, .. } => self.collect_entries(*child, out),
            }
        }
    }

    // -- Internal: find entry ------------------------------------------------

    /// Finds the leaf node and entry index for a given (bbox, data) pair.
    fn find_entry(&self, bbox: &BoundingBox, data: &T) -> Option<(NodeId, usize)>
    where
        T: PartialEq,
    {
        let root_id = self.root?;
        self.find_entry_recursive(root_id, bbox, data)
    }

    fn find_entry_recursive(
        &self,
        node_id: NodeId,
        bbox: &BoundingBox,
        data: &T,
    ) -> Option<(NodeId, usize)>
    where
        T: PartialEq,
    {
        let node = &self.nodes[node_id];
        if node.is_leaf {
            for (i, entry) in node.entries.iter().enumerate() {
                if let Entry::Leaf { bbox: b, data: d } = entry {
                    if b == bbox && d == data {
                        return Some((node_id, i));
                    }
                }
            }
            None
        } else {
            for entry in &node.entries {
                if entry.bbox().intersects(bbox) {
                    if let Entry::Internal { child, .. } = entry {
                        if let Some(result) = self.find_entry_recursive(*child, bbox, data) {
                            return Some(result);
                        }
                    }
                }
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Bulk-load (STR packing)
// ---------------------------------------------------------------------------

impl<T: Clone> STRtree<T> {
    /// Bulk-loads items into the tree using STR (Sort-Tile-Recursive) packing.
    ///
    /// STR packing produces a tree with zero overlap between sibling nodes
    /// at each level, giving optimal query performance for static datasets.
    /// This is the preferred build method when all items are known upfront.
    ///
    /// Matches QGIS `QgsSpatialIndex` bulk population (which uses the
    /// spatialindex library's bulk-load).
    pub fn bulk_load(items: Vec<(BoundingBox, T)>) -> Self {
        let mut tree = Self::with_defaults();
        tree.bulk_load_into(items);
        tree
    }

    /// Bulk-loads items into an existing tree, replacing all current contents.
    pub fn bulk_load_into(&mut self, items: Vec<(BoundingBox, T)>) {
        self.size = items.len();
        if items.is_empty() {
            self.root = None;
            return;
        }

        // STR packing: sort by x-center, split into vertical slices,
        // sort each slice by y-center, pack into leaf nodes.
        let mut sorted = items;
        sorted.sort_by(|a, b| {
            a.0.center_x()
                .partial_cmp(&b.0.center_x())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let num_leaves = (sorted.len() + self.node_capacity - 1) / self.node_capacity;
        let slice_size = (sorted.len() + num_leaves - 1) / num_leaves;

        let mut current_level: Vec<NodeId> = Vec::new();
        for slice in sorted.chunks(slice_size) {
            // Sort slice by y-center.
            let mut slice_vec: Vec<_> = slice.to_vec();
            slice_vec.sort_by(|a, b| {
                a.0.center_y()
                    .partial_cmp(&b.0.center_y())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Pack into leaf nodes.
            for chunk in slice_vec.chunks(self.node_capacity) {
                let node_id = self.alloc_node(true);
                for (bbox, data) in chunk {
                    self.nodes[node_id].entries.push(Entry::Leaf {
                        bbox: *bbox,
                        data: data.clone(),
                    });
                }
                self.update_bbox(node_id);
                current_level.push(node_id);
            }
        }

        // Build internal levels bottom-up.
        while current_level.len() > 1 {
            let level_items: Vec<(BoundingBox, NodeId)> = current_level
                .iter()
                .map(|&id| (self.nodes[id].bbox, id))
                .collect();

            let mut next_level: Vec<NodeId> = Vec::new();
            for chunk in level_items.chunks(self.node_capacity) {
                let node_id = self.alloc_node(false);
                for &(bbox, child_id) in chunk {
                    self.nodes[node_id].entries.push(Entry::Internal {
                        bbox,
                        child: child_id,
                    });
                }
                self.update_bbox(node_id);
                next_level.push(node_id);
            }
            current_level = next_level;
        }

        self.root = current_level.first().copied();
    }

    /// Inserts a single item into the tree.
    ///
    /// Uses least-enlargement leaf choice and quadratic split for overflow.
    /// Matches QGIS `QgsSpatialIndex::insertFeature`.
    pub fn insert(&mut self, bbox: BoundingBox, data: T) {
        // If tree is empty, create a root leaf.
        if self.root.is_none() {
            let node_id = self.alloc_node(true);
            self.nodes[node_id].entries.push(Entry::Leaf { bbox, data });
            self.nodes[node_id].bbox = bbox;
            self.root = Some(node_id);
            self.size = 1;
            return;
        }

        // Choose a leaf to insert into.
        let path = self.choose_leaf(bbox);
        let leaf_id = *path.last().unwrap();

        // Insert into the leaf.
        self.nodes[leaf_id].entries.push(Entry::Leaf { bbox, data });
        self.size += 1;

        // Handle overflow, otherwise propagate the (possibly grown) bbox up
        // the path so ancestor entries never go stale.
        if self.nodes[leaf_id].entries.len() > self.node_capacity {
            self.split_and_propagate(path);
        } else {
            self.refresh_path_entries(&path);
        }
    }

    /// Removes an item from the tree.
    ///
    /// Finds the item by (bbox, data) equality, removes it, and updates
    /// bboxes up the tree. Uses lazy condensing — nodes that underflow are
    /// not immediately rebuilt, keeping the operation O(log n). The tree
    /// may be rebuilt periodically if many removes cause degradation.
    ///
    /// Returns `true` if the item was found and removed.
    /// Matches QGIS `QgsSpatialIndex::deleteFeature`.
    pub fn remove(&mut self, bbox: &BoundingBox, data: &T) -> bool
    where
        T: PartialEq,
    {
        if let Some((leaf_id, entry_idx)) = self.find_entry(bbox, data) {
            self.nodes[leaf_id].entries.remove(entry_idx);
            self.size -= 1;

            // Update bboxes up the tree (bottom-up, syncing each parent's
            // entry for its child to the child's current bbox).
            let path = self.find_path_to_node(leaf_id);
            self.refresh_path_entries(&path);

            true
        } else {
            false
        }
    }

    // -- Insert helpers -------------------------------------------------------

    /// Chooses the leaf node that requires the least enlargement to
    /// accommodate `bbox`. Returns the path from root to leaf.
    fn choose_leaf(&self, bbox: BoundingBox) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut node_id = self.root.unwrap();

        loop {
            path.push(node_id);
            let node = &self.nodes[node_id];
            if node.is_leaf {
                break;
            }

            // Choose the child that requires the least enlargement.
            let mut best_child = 0;
            let mut min_enlargement = f64::INFINITY;
            let mut min_area = f64::INFINITY;

            for entry in &node.entries {
                let enlargement = entry.bbox().expanded_area(&bbox) - entry.bbox().area();
                if enlargement < min_enlargement
                    || (enlargement == min_enlargement && entry.bbox().area() < min_area)
                {
                    min_enlargement = enlargement;
                    min_area = entry.bbox().area();
                    best_child = match entry {
                        Entry::Internal { child, .. } => *child,
                        _ => unreachable!(),
                    };
                }
            }

            node_id = best_child;
        }

        path
    }

    /// Splits a node and propagates the split up the tree.
    fn split_and_propagate(&mut self, path: Vec<NodeId>) {
        let mut new_node: Option<NodeId> = None;

        // Walk up the path, splitting nodes as needed.
        for i in (0..path.len()).rev() {
            let node_id = path[i];

            if let Some(new_id) = new_node {
                // Insert the new node into this parent.
                let new_bbox = self.nodes[new_id].bbox;
                self.nodes[node_id].entries.push(Entry::Internal {
                    bbox: new_bbox,
                    child: new_id,
                });
                self.update_bbox(node_id);
            }

            if self.nodes[node_id].entries.len() > self.node_capacity {
                let split_id = self.quadratic_split(node_id);
                // Update the parent's entry for this node (bbox changed by split),
                // then recompute the parent's bbox so subsequent splits see correct data.
                if i > 0 {
                    let parent_id = path[i - 1];
                    let new_bbox = self.nodes[node_id].bbox;
                    for entry in &mut self.nodes[parent_id].entries {
                        if let Entry::Internal { bbox, child } = entry {
                            if *child == node_id {
                                *bbox = new_bbox;
                            }
                        }
                    }
                    self.update_bbox(parent_id);
                }
                new_node = Some(split_id);
            } else {
                new_node = None;
            }
        }

        // If the root was split, create a new root.
        if let Some(new_id) = new_node {
            let root_id = self.root.unwrap();
            let new_root = self.alloc_node(false);
            let root_bbox = self.nodes[root_id].bbox;
            let new_bbox = self.nodes[new_id].bbox;
            self.nodes[new_root].entries.push(Entry::Internal {
                bbox: root_bbox,
                child: root_id,
            });
            self.nodes[new_root].entries.push(Entry::Internal {
                bbox: new_bbox,
                child: new_id,
            });
            self.update_bbox(new_root);
            self.root = Some(new_root);
        }

        // Splitting a leaf grows its parent's bbox even when the parent does
        // not itself overflow; refresh the whole path bottom-up so no ancestor
        // keeps a stale entry and bbox-pruned searches cannot lose items.
        self.refresh_path_entries(&path);
    }

    /// Splits a node using the quadratic split algorithm.
    ///
    /// Picks two seed entries that would waste the most area if grouped
    /// together, then distributes remaining entries to minimize area
    /// enlargement.
    fn quadratic_split(&mut self, node_id: NodeId) -> NodeId {
        let is_leaf = self.nodes[node_id].is_leaf;
        let entries = std::mem::take(&mut self.nodes[node_id].entries);

        // Pick two seeds.
        let (seed1, seed2) = self.pick_seeds(&entries);

        let mut group1: Vec<Entry<T>> = Vec::new();
        let mut group2: Vec<Entry<T>> = Vec::new();

        // Move seeds out.
        let mut remaining: Vec<Entry<T>> = Vec::with_capacity(entries.len() - 2);
        for (i, entry) in entries.into_iter().enumerate() {
            if i == seed1 {
                group1.push(entry);
            } else if i == seed2 {
                group2.push(entry);
            } else {
                remaining.push(entry);
            }
        }

        let mut bbox1 = group1[0].bbox();
        let mut bbox2 = group2[0].bbox();

        let min_entries = self.min_entries();

        while !remaining.is_empty() {
            // If one group needs all remaining entries to meet minimum, assign them.
            // The group bbox must absorb every appended entry, otherwise the split
            // node's bbox under-covers its children and later bbox-pruned searches
            // (query, remove, choose_leaf) lose those items.
            if group1.len() + remaining.len() == min_entries {
                for entry in &remaining {
                    bbox1 = bbox1.union(&entry.bbox());
                }
                group1.append(&mut remaining);
                break;
            }
            if group2.len() + remaining.len() == min_entries {
                for entry in &remaining {
                    bbox2 = bbox2.union(&entry.bbox());
                }
                group2.append(&mut remaining);
                break;
            }

            // Pick the entry with the maximum difference in area enlargement.
            let mut best_idx = 0;
            let mut best_diff = f64::NEG_INFINITY;
            let mut best_group = 0;

            for (i, entry) in remaining.iter().enumerate() {
                let d1 = bbox1.expanded_area(&entry.bbox()) - bbox1.area();
                let d2 = bbox2.expanded_area(&entry.bbox()) - bbox2.area();
                let diff = (d1 - d2).abs();
                if diff > best_diff {
                    best_diff = diff;
                    best_idx = i;
                    best_group = if d1 < d2 { 1 } else { 2 };
                }
            }

            let entry = remaining.remove(best_idx);
            if best_group == 1 {
                bbox1 = bbox1.union(&entry.bbox());
                group1.push(entry);
            } else {
                bbox2 = bbox2.union(&entry.bbox());
                group2.push(entry);
            }
        }

        // Update the original node with group1.
        self.nodes[node_id].entries = group1;
        self.nodes[node_id].bbox = bbox1;

        // Create a new node for group2.
        let new_id = self.alloc_node(is_leaf);
        self.nodes[new_id].entries = group2;
        self.nodes[new_id].bbox = bbox2;

        new_id
    }

    /// Picks two seed entries for the quadratic split: the pair that would
    /// waste the most area if grouped together.
    fn pick_seeds(&self, entries: &[Entry<T>]) -> (usize, usize) {
        let mut seed1 = 0;
        let mut seed2 = 1;
        let mut max_waste = f64::NEG_INFINITY;

        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let waste = entries[i].bbox().expanded_area(&entries[j].bbox())
                    - entries[i].bbox().area()
                    - entries[j].bbox().area();
                if waste > max_waste {
                    max_waste = waste;
                    seed1 = i;
                    seed2 = j;
                }
            }
        }

        (seed1, seed2)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction -------------------------------------------------------

    #[test]
    fn new_tree_is_empty() {
        let tree: STRtree<i32> = STRtree::new(10);
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert_eq!(tree.node_capacity(), 10);
    }

    #[test]
    fn new_tree_clamps_capacity() {
        let tree: STRtree<i32> = STRtree::new(0);
        assert_eq!(tree.node_capacity(), 2);
    }

    #[test]
    fn with_defaults_capacity() {
        let tree: STRtree<i32> = STRtree::with_defaults();
        assert_eq!(tree.node_capacity(), 10);
    }

    // -- Bulk-load -----------------------------------------------------------

    #[test]
    fn bulk_load_empty() {
        let items: Vec<(BoundingBox, i32)> = vec![];
        let tree = STRtree::bulk_load(items);
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn bulk_load_single_item() {
        let items = vec![(BoundingBox::new(0.0, 0.0, 1.0, 1.0), 42)];
        let tree = STRtree::bulk_load(items);
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());
    }

    #[test]
    fn bulk_load_preserves_all_items() {
        let items: Vec<(BoundingBox, i32)> = (0..100)
            .map(|i| {
                let x = i as f64 * 2.0;
                let y = i as f64 * 2.0;
                (BoundingBox::new(x, y, x + 1.0, y + 1.0), i)
            })
            .collect();
        let tree = STRtree::bulk_load(items);
        assert_eq!(tree.len(), 100);

        // Every item should be queryable.
        for i in 0..100 {
            let x = i as f64 * 2.0;
            let y = i as f64 * 2.0;
            let query = BoundingBox::new(x, y, x + 1.0, y + 1.0);
            let results = tree.query_bbox(&query);
            assert_eq!(results.len(), 1, "item {i} not found");
            assert_eq!(*results[0], i);
        }
    }

    #[test]
    fn bulk_load_query_no_false_positives() {
        let items = vec![
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "a"),
            (BoundingBox::new(5.0, 5.0, 6.0, 6.0), "b"),
            (BoundingBox::new(10.0, 10.0, 11.0, 11.0), "c"),
        ];
        let tree = STRtree::bulk_load(items);

        let results = tree.query_bbox(&BoundingBox::new(0.5, 0.5, 0.6, 0.6));
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0], "a");

        let results = tree.query_bbox(&BoundingBox::new(7.0, 7.0, 8.0, 8.0));
        assert!(results.is_empty());
    }

    // -- Insert --------------------------------------------------------------

    #[test]
    fn insert_into_empty_tree() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        tree.insert(BoundingBox::new(0.0, 0.0, 1.0, 1.0), 42);
        assert_eq!(tree.len(), 1);
        let results = tree.query_bbox(&BoundingBox::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0], 42);
    }

    #[test]
    fn insert_multiple_items() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        for i in 0..20 {
            let x = i as f64 * 2.0;
            let y = i as f64 * 2.0;
            tree.insert(BoundingBox::new(x, y, x + 1.0, y + 1.0), i);
        }
        assert_eq!(tree.len(), 20);

        // Query a subset — items 5,6,7,8,9,10 (item 10 touches at edge).
        let results = tree.query_bbox(&BoundingBox::new(10.0, 10.0, 20.0, 20.0));
        assert_eq!(results.len(), 6);
    }

    #[test]
    fn insert_causes_split() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        // Insert enough items to force multiple splits.
        for i in 0..100 {
            let x = i as f64 * 2.0;
            tree.insert(BoundingBox::new(x, 0.0, x + 1.0, 1.0), i);
        }
        assert_eq!(tree.len(), 100);

        // All items should still be queryable.
        for i in 0..100 {
            let x = i as f64 * 2.0;
            let results = tree.query_bbox(&BoundingBox::new(x, 0.0, x + 1.0, 1.0));
            assert_eq!(results.len(), 1, "item {i} not found after split");
            assert_eq!(*results[0], i);
        }
    }

    #[test]
    fn insert_that_grows_leaf_without_split_propagates_bbox() {
        // Regression (qgis-f5y): after a split creates an internal root,
        // inserting into a leaf whose bbox grows but does not overflow must
        // still refresh the parent's stored entry for that leaf. Previously
        // the parent kept the old (smaller) bbox, so a bbox-pruned query
        // silently lost the newly inserted item.
        let mut tree: STRtree<i32> = STRtree::new(3);
        // Force a split: root leaf overflows at the 4th insert.
        for i in 0..4 {
            tree.insert(BoundingBox::new(i as f64 * 10.0, 0.0, i as f64 * 10.0 + 1.0, 1.0), i);
        }
        assert!(!tree.nodes[tree.root.unwrap()].is_leaf);
        // This insert grows the right-hand leaf's bbox without splitting it.
        tree.insert(BoundingBox::new(40.0, 0.0, 41.0, 1.0), 4);

        // Every item, including the last, must be findable.
        for i in 0..5 {
            let x = i as f64 * 10.0;
            let results = tree.query_bbox(&BoundingBox::new(x, 0.0, x + 1.0, 1.0));
            assert_eq!(results.len(), 1, "item {i} lost after non-splitting insert");
            assert_eq!(*results[0], i);
        }
    }

    // -- Remove --------------------------------------------------------------

    #[test]
    fn remove_existing_item() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        tree.insert(BoundingBox::new(0.0, 0.0, 1.0, 1.0), 42);
        assert_eq!(tree.len(), 1);

        let removed = tree.remove(&BoundingBox::new(0.0, 0.0, 1.0, 1.0), &42);
        assert!(removed);
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
    }

    #[test]
    fn remove_nonexistent_item() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        tree.insert(BoundingBox::new(0.0, 0.0, 1.0, 1.0), 42);

        let removed = tree.remove(&BoundingBox::new(5.0, 5.0, 6.0, 6.0), &99);
        assert!(!removed);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn remove_from_bulk_loaded_tree() {
        let items: Vec<(BoundingBox, i32)> = (0..50)
            .map(|i| {
                let x = i as f64 * 2.0;
                let y = i as f64 * 2.0;
                (BoundingBox::new(x, y, x + 1.0, y + 1.0), i)
            })
            .collect();
        let mut tree = STRtree::bulk_load(items);
        assert_eq!(tree.len(), 50);

        // Remove every other item.
        for i in (0..50).step_by(2) {
            let x = i as f64 * 2.0;
            let y = i as f64 * 2.0;
            let bbox = BoundingBox::new(x, y, x + 1.0, y + 1.0);
            assert!(tree.remove(&bbox, &i));
        }
        assert_eq!(tree.len(), 25);

        // Removed items should not be found.
        for i in (0..50).step_by(2) {
            let x = i as f64 * 2.0;
            let y = i as f64 * 2.0;
            let bbox = BoundingBox::new(x, y, x + 1.0, y + 1.0);
            assert!(tree.query_bbox(&bbox).is_empty());
        }

        // Remaining items should still be found.
        for i in (1..50).step_by(2) {
            let x = i as f64 * 2.0;
            let y = i as f64 * 2.0;
            let bbox = BoundingBox::new(x, y, x + 1.0, y + 1.0);
            let results = tree.query_bbox(&bbox);
            assert_eq!(results.len(), 1);
            assert_eq!(*results[0], i);
        }
    }

    // -- Query ---------------------------------------------------------------

    #[test]
    fn query_empty_tree() {
        let tree: STRtree<i32> = STRtree::new(4);
        let results = tree.query_bbox(&BoundingBox::new(0.0, 0.0, 10.0, 10.0));
        assert!(results.is_empty());
    }

    #[test]
    fn query_returns_all_intersecting() {
        let items = vec![
            (BoundingBox::new(0.0, 0.0, 2.0, 2.0), 1),
            (BoundingBox::new(1.0, 1.0, 3.0, 3.0), 2),
            (BoundingBox::new(5.0, 5.0, 6.0, 6.0), 3),
        ];
        let tree = STRtree::bulk_load(items);

        let results = tree.query_bbox(&BoundingBox::new(0.5, 0.5, 2.5, 2.5));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_edge_contact_counts_as_intersection() {
        let items = vec![(BoundingBox::new(0.0, 0.0, 1.0, 1.0), "a")];
        let tree = STRtree::bulk_load(items);

        // Touching at x=1.0 edge.
        let results = tree.query_bbox(&BoundingBox::new(1.0, 0.0, 2.0, 1.0));
        assert_eq!(results.len(), 1);
    }

    // -- Nearest neighbor ----------------------------------------------------

    #[test]
    fn nearest_neighbor_empty_tree() {
        let tree: STRtree<i32> = STRtree::new(4);
        assert!(tree.nearest_neighbor(0.0, 0.0).is_none());
    }

    #[test]
    fn nearest_neighbor_single_item() {
        let items = vec![(BoundingBox::new(0.0, 0.0, 1.0, 1.0), 42)];
        let tree = STRtree::bulk_load(items);
        let result = tree.nearest_neighbor(5.0, 5.0);
        assert_eq!(*result.unwrap(), 42);
    }

    #[test]
    fn nearest_neighbor_returns_closest() {
        let items = vec![
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "far"),
            (BoundingBox::new(10.0, 10.0, 11.0, 11.0), "near"),
            (BoundingBox::new(20.0, 20.0, 21.0, 21.0), "farthest"),
        ];
        let tree = STRtree::bulk_load(items);

        let result = tree.nearest_neighbor(10.5, 10.5);
        assert_eq!(*result.unwrap(), "near");
    }

    #[test]
    fn nearest_neighbor_point_inside_box() {
        let items = vec![(BoundingBox::new(0.0, 0.0, 10.0, 10.0), "inside")];
        let tree = STRtree::bulk_load(items);

        let result = tree.nearest_neighbor(5.0, 5.0);
        assert_eq!(*result.unwrap(), "inside");
    }

    // -- Mixed operations ----------------------------------------------------

    #[test]
    fn insert_after_remove() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        tree.insert(BoundingBox::new(0.0, 0.0, 1.0, 1.0), 1);
        tree.insert(BoundingBox::new(2.0, 2.0, 3.0, 3.0), 2);
        tree.remove(&BoundingBox::new(0.0, 0.0, 1.0, 1.0), &1);
        tree.insert(BoundingBox::new(4.0, 4.0, 5.0, 5.0), 3);

        assert_eq!(tree.len(), 2);
        assert!(tree.query_bbox(&BoundingBox::new(0.0, 0.0, 1.0, 1.0)).is_empty());
        assert_eq!(tree.query_bbox(&BoundingBox::new(2.0, 2.0, 3.0, 3.0)).len(), 1);
        assert_eq!(tree.query_bbox(&BoundingBox::new(4.0, 4.0, 5.0, 5.0)).len(), 1);
    }

    // -- Large-scale correctness ---------------------------------------------

    #[test]
    fn bulk_load_large_dataset() {
        let items: Vec<(BoundingBox, i32)> = (0..10_000)
            .map(|i| {
                let x = (i % 100) as f64 * 2.0;
                let y = (i / 100) as f64 * 2.0;
                (BoundingBox::new(x, y, x + 1.0, y + 1.0), i)
            })
            .collect();
        let tree = STRtree::bulk_load(items);
        assert_eq!(tree.len(), 10_000);

        // Query a small region.
        let results = tree.query_bbox(&BoundingBox::new(20.0, 20.0, 40.0, 40.0));
        assert_eq!(results.len(), 121); // 11x11 grid of 1.0-sized boxes (edge contact)
    }

    #[test]
    fn insert_many_and_query() {
        let mut tree: STRtree<i32> = STRtree::new(8);
        for i in 0..1000 {
            let x = (i % 32) as f64 * 10.0;
            let y = (i / 32) as f64 * 10.0;
            tree.insert(BoundingBox::new(x, y, x + 1.0, y + 1.0), i);
        }
        assert_eq!(tree.len(), 1000);

        let results = tree.query_bbox(&BoundingBox::new(50.0, 50.0, 160.0, 160.0));
        // Grid step is 10 over 32 columns/rows, so the query spans cell
        // minima 50..=160 inclusive (x=160's box touches query.max_x=160,
        // and edge contact counts as intersection) => 12x12 = 144.
        assert_eq!(results.len(), 144);
    }

    // -- Duplicate bboxes -----------------------------------------------------

    #[test]
    fn duplicate_bboxes_all_queryable() {
        let items = vec![
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "a"),
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "b"),
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "c"),
        ];
        let tree = STRtree::bulk_load(items);

        let results = tree.query_bbox(&BoundingBox::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(results.len(), 3);
    }

    // -- STR packing structure -----------------------------------------------

    #[test]
    fn bulk_load_creates_valid_tree() {
        let items: Vec<(BoundingBox, i32)> = (0..100)
            .map(|i| (BoundingBox::new(i as f64, i as f64, (i + 1) as f64, (i + 1) as f64), i))
            .collect();
        let tree = STRtree::bulk_load(items);

        // The tree should have a root.
        assert!(tree.root.is_some());

        // All items should be reachable.
        assert_eq!(tree.len(), 100);
    }

    // -- OrderedF64 -----------------------------------------------------------

    #[test]
    fn ordered_f64_nan_is_smallest() {
        assert!(OrderedF64(f64::NAN) < OrderedF64(0.0));
        assert!(OrderedF64(0.0) < OrderedF64(1.0));
        assert!(OrderedF64(f64::NAN) < OrderedF64(f64::INFINITY));
    }

    // -- Insert with split propagation ---------------------------------------

    #[test]
    fn insert_splits_propagate_correctly() {
        let mut tree: STRtree<i32> = STRtree::new(3);
        // Insert items that will force multiple levels of splitting.
        for i in 0..50 {
            let x = (i % 10) as f64 * 10.0;
            let y = (i / 10) as f64 * 10.0;
            tree.insert(BoundingBox::new(x, y, x + 1.0, y + 1.0), i);
        }
        assert_eq!(tree.len(), 50);

        // Verify all items are still queryable after splits.
        for i in 0..50 {
            let x = (i % 10) as f64 * 10.0;
            let y = (i / 10) as f64 * 10.0;
            let results = tree.query_bbox(&BoundingBox::new(x, y, x + 1.0, y + 1.0));
            assert_eq!(results.len(), 1, "item {i} not found after splits");
            assert_eq!(*results[0], i);
        }
    }

    #[test]
    fn debug_small_split() {
        let mut tree: STRtree<i32> = STRtree::new(3);
        // Insert 10 items, one at a time, checking after each.
        for i in 0..10 {
            let x = (i % 5) as f64 * 10.0;
            let y = (i / 5) as f64 * 10.0;
            tree.insert(BoundingBox::new(x, y, x + 1.0, y + 1.0), i);

            // After each insert, verify all previous items are still findable.
            for j in 0..=i {
                let xj = (j % 5) as f64 * 10.0;
                let yj = (j / 5) as f64 * 10.0;
                let results = tree.query_bbox(&BoundingBox::new(xj, yj, xj + 1.0, yj + 1.0));
                assert_eq!(results.len(), 1, "item {j} lost after inserting item {i}");
                assert_eq!(*results[0], j);
            }
        }
    }

    // -- Remove with many items ----------------------------------------------

    #[test]
    fn remove_many_items_keeps_tree_consistent() {
        let mut tree: STRtree<i32> = STRtree::new(4);
        for i in 0..100 {
            let x = (i % 10) as f64 * 10.0;
            let y = (i / 10) as f64 * 10.0;
            tree.insert(BoundingBox::new(x, y, x + 1.0, y + 1.0), i);
        }
        assert_eq!(tree.len(), 100);

        // Remove all items.
        for i in 0..100 {
            let x = (i % 10) as f64 * 10.0;
            let y = (i / 10) as f64 * 10.0;
            assert!(tree.remove(&BoundingBox::new(x, y, x + 1.0, y + 1.0), &i));
        }
        assert_eq!(tree.len(), 0);

        // Tree should be empty.
        let results = tree.query_bbox(&BoundingBox::new(-1.0, -1.0, 200.0, 200.0));
        assert!(results.is_empty());
    }

    // -- Query with negative coordinates --------------------------------------

    #[test]
    fn query_negative_coordinates() {
        let items = vec![
            (BoundingBox::new(-10.0, -10.0, -5.0, -5.0), "a"),
            (BoundingBox::new(5.0, 5.0, 10.0, 10.0), "b"),
        ];
        let tree = STRtree::bulk_load(items);

        let results = tree.query_bbox(&BoundingBox::new(-8.0, -8.0, -6.0, -6.0));
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0], "a");

        let results = tree.query_bbox(&BoundingBox::new(-1.0, -1.0, 1.0, 1.0));
        assert!(results.is_empty());
    }

    // -- Nearest neighbor with ties -------------------------------------------

    #[test]
    fn nearest_neighbor_returns_some_when_tied() {
        let items = vec![
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "a"),
            (BoundingBox::new(0.0, 0.0, 1.0, 1.0), "b"),
        ];
        let tree = STRtree::bulk_load(items);

        let result = tree.nearest_neighbor(0.5, 0.5);
        assert!(result.is_some());
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    fn dump_tree(tree: &STRtree<i32>, label: &str) {
        eprintln!("=== {label} ===");
        for (id, node) in tree.nodes.iter().enumerate() {
            eprintln!(
                "node {id} leaf={} bbox=({}, {}, {}, {}) entries={}",
                node.is_leaf,
                node.bbox.min_x,
                node.bbox.min_y,
                node.bbox.max_x,
                node.bbox.max_y,
                node.entries.len()
            );
            for entry in &node.entries {
                match entry {
                    Entry::Leaf { bbox, data } => eprintln!(
                        "  leaf data={} bbox=({}, {}, {}, {})",
                        data, bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y
                    ),
                    Entry::Internal { bbox, child } => eprintln!(
                        "  internal child={} bbox=({}, {}, {}, {})",
                        child, bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y
                    ),
                }
            }
        }
        eprintln!("root={:?} size={}", tree.root, tree.size);
    }

    #[test]
    fn debug_trace_small_split() {
        let mut tree: STRtree<i32> = STRtree::new(3);
        for i in 0..10 {
            let x = (i % 5) as f64 * 10.0;
            let y = (i / 5) as f64 * 10.0;
            tree.insert(BoundingBox::new(x, y, x + 1.0, y + 1.0), i);
            dump_tree(&tree, &format!("after insert {i}"));
            for j in 0..=i {
                let xj = (j % 5) as f64 * 10.0;
                let yj = (j / 5) as f64 * 10.0;
                let results = tree.query_bbox(&BoundingBox::new(xj, yj, xj + 1.0, yj + 1.0));
                if results.len() != 1 || *results[0] != j {
                    eprintln!("LOST item {j} after insert {i}: results={results:?}");
                    dump_tree(&tree, &format!("LOST {j} after insert {i}"));
                    panic!("lost");
                }
            }
        }
    }
}
