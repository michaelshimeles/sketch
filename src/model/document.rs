//! The document arena: nodes keyed by id plus ordered child lists.

use super::node::{Node, NodeId};
use crate::geom::{Rect, V2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub nodes: HashMap<NodeId, Node>,
    /// Top-level nodes in paint order, bottom to top.
    pub root: Vec<NodeId>,
    pub next_id: u64,
    #[serde(default)]
    pub comments: Vec<super::Comment>,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            name: "Untitled".into(),
            nodes: HashMap::new(),
            root: Vec::new(),
            next_id: 1,
            comments: Vec::new(),
        }
    }
}

impl Document {
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn children(&self, parent: Option<NodeId>) -> &[NodeId] {
        match parent {
            None => &self.root,
            Some(p) => self
                .nodes
                .get(&p)
                .map(|n| n.children.as_slice())
                .unwrap_or(&[]),
        }
    }

    pub fn children_mut(&mut self, parent: Option<NodeId>) -> Option<&mut Vec<NodeId>> {
        match parent {
            None => Some(&mut self.root),
            Some(p) => self.nodes.get_mut(&p).map(|n| &mut n.children),
        }
    }

    /// World-space position of a node's local coordinate origin's parent space.
    pub fn parent_origin(&self, parent: Option<NodeId>) -> V2 {
        let mut o = V2::ZERO;
        let mut cur = parent;
        while let Some(id) = cur {
            let Some(n) = self.nodes.get(&id) else { break };
            o = o + n.rect.min();
            cur = n.parent;
        }
        o
    }

    pub fn world_rect(&self, id: NodeId) -> Option<Rect> {
        let n = self.nodes.get(&id)?;
        Some(n.rect.translate(self.parent_origin(n.parent)))
    }

    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.nodes.get(&id).and_then(|n| n.parent);
        while let Some(p) = cur {
            out.push(p);
            cur = self.nodes.get(&p).and_then(|n| n.parent);
        }
        out
    }

    pub fn is_ancestor(&self, ancestor: NodeId, of: NodeId) -> bool {
        self.ancestors(of).contains(&ancestor)
    }

    /// The node and all its descendants, parent first.
    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            if let Some(n) = self.nodes.get(&out[i]) {
                out.extend(n.children.iter().copied());
            }
            i += 1;
        }
        out
    }

    /// All nodes in paint order (depth-first, parents before children).
    pub fn paint_order(&self) -> Vec<NodeId> {
        fn walk(doc: &Document, ids: &[NodeId], out: &mut Vec<NodeId>) {
            for id in ids {
                out.push(*id);
                if let Some(n) = doc.nodes.get(id) {
                    walk(doc, &n.children, out);
                }
            }
        }
        let mut out = Vec::with_capacity(self.nodes.len());
        walk(self, &self.root, &mut out);
        out
    }

    /// Whether a node and all its ancestors are visible.
    pub fn is_effectively_visible(&self, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            match self.nodes.get(&c) {
                Some(n) if n.visible => cur = n.parent,
                _ => return false,
            }
        }
        true
    }

    pub fn is_effectively_locked(&self, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            match self.nodes.get(&c) {
                Some(n) if n.locked => return true,
                Some(n) => cur = n.parent,
                None => return false,
            }
        }
        false
    }

    /// Bounds of all visible content in world space.
    pub fn content_bounds(&self) -> Option<Rect> {
        let rects: Vec<Rect> = self
            .root
            .iter()
            .filter(|id| self.nodes.get(id).is_some_and(|n| n.visible))
            .filter_map(|id| self.world_rect(*id))
            .collect();
        Rect::union_all(&rects)
    }

    /// Structural invariants, used by tests and after loading files.
    pub fn validate(&self) -> Result<(), String> {
        let mut comment_ids = std::collections::HashSet::new();
        for c in &self.comments {
            if c.id.is_empty()
                || !comment_ids.insert(&c.id)
                || c.body.trim().is_empty()
                || c.body.chars().count() > 10_000
                || !c.relative.x.is_finite()
                || !c.relative.y.is_finite()
                || !(0.0..=1.0).contains(&c.relative.x)
                || !(0.0..=1.0).contains(&c.relative.y)
                || !c.fallback.x.is_finite()
                || !c.fallback.y.is_finite()
            {
                return Err("invalid comment metadata".into());
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut check = |ids: &[NodeId], parent: Option<NodeId>| -> Result<(), String> {
            for id in ids {
                let n = self.nodes.get(id).ok_or(format!("missing node {id}"))?;
                if n.parent != parent {
                    return Err(format!(
                        "node {id} has parent {:?}, expected {:?}",
                        n.parent, parent
                    ));
                }
                if !seen.insert(*id) {
                    return Err(format!("node {id} appears twice"));
                }
            }
            Ok(())
        };
        check(&self.root, None)?;
        for n in self.nodes.values() {
            check(&n.children, Some(n.id))?;
            if n.id.0 >= self.next_id {
                return Err(format!("node {} >= next_id {}", n.id, self.next_id));
            }
        }
        if seen.len() != self.nodes.len() {
            return Err(format!(
                "{} nodes unreachable",
                self.nodes.len() - seen.len()
            ));
        }
        Ok(())
    }
}
