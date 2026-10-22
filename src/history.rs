//! Command-pattern undo/redo built on recorded transactions.
//!
//! Every edit runs inside a [`Tx`], which lazily snapshots the *before* state of each node
//! (and the root list) the first time it is touched. Committing captures the *after* state
//! and pushes a [`Change`] that can be applied in either direction.

use crate::geom::V2;
use crate::model::{Comment, Document, Node, NodeId};
use std::collections::HashMap;

const MAX_DEPTH: usize = 500;

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    id: NodeId,
    before: Option<Node>,
    after: Option<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub label: String,
    entries: Vec<Entry>,
    root: Option<(Vec<NodeId>, Vec<NodeId>)>,
    next_id: (u64, u64),
    coalesce_key: Option<String>,
    comments: Option<(Vec<Comment>, Vec<Comment>)>,
}

impl Change {
    fn apply(&self, doc: &mut Document, forward: bool) {
        for e in &self.entries {
            let state = if forward { &e.after } else { &e.before };
            match state {
                Some(n) => {
                    doc.nodes.insert(e.id, n.clone());
                }
                None => {
                    doc.nodes.remove(&e.id);
                }
            }
        }
        if let Some((before, after)) = &self.root {
            doc.root = if forward {
                after.clone()
            } else {
                before.clone()
            };
        }
        if let Some((before, after)) = &self.comments {
            doc.comments = if forward {
                after.clone()
            } else {
                before.clone()
            };
        }
        doc.next_id = if forward {
            self.next_id.1
        } else {
            self.next_id.0
        };
    }
}

/// Before-state captured by an open transaction.
#[derive(Debug, Default)]
pub struct Recording {
    label: String,
    before: HashMap<NodeId, Option<Node>>,
    order: Vec<NodeId>,
    root_before: Option<Vec<NodeId>>,
    next_id_before: u64,
    comments_before: Option<Vec<Comment>>,
}

impl Recording {
    /// Nodes touched so far, in first-touch order.
    pub fn touched(&self) -> &[NodeId] {
        &self.order
    }
    /// The state of a touched node when the transaction began (None if it didn't exist).
    pub fn before(&self, id: NodeId) -> Option<&Option<Node>> {
        self.before.get(&id)
    }
    pub fn new(label: impl Into<String>, doc: &Document) -> Recording {
        Recording {
            label: label.into(),
            next_id_before: doc.next_id,
            ..Default::default()
        }
    }
}

/// A mutable view of the document that records what it touches.
pub struct Tx<'a> {
    pub doc: &'a mut Document,
    rec: &'a mut Recording,
}

impl<'a> Tx<'a> {
    pub fn new(doc: &'a mut Document, rec: &'a mut Recording) -> Tx<'a> {
        Tx { doc, rec }
    }

    fn touch(&mut self, id: NodeId) {
        if !self.rec.before.contains_key(&id) {
            self.rec.before.insert(id, self.doc.nodes.get(&id).cloned());
            self.rec.order.push(id);
        }
    }

    fn touch_parent_list(&mut self, parent: Option<NodeId>) {
        match parent {
            None => {
                if self.rec.root_before.is_none() {
                    self.rec.root_before = Some(self.doc.root.clone());
                }
            }
            Some(p) => self.touch(p),
        }
    }

    pub fn doc(&self) -> &Document {
        self.doc
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if !self.doc.nodes.contains_key(&id) {
            return None;
        }
        self.touch(id);
        self.doc.nodes.get_mut(&id)
    }

    pub fn comments_mut(&mut self) -> &mut Vec<Comment> {
        if self.rec.comments_before.is_none() {
            self.rec.comments_before = Some(self.doc.comments.clone());
        }
        &mut self.doc.comments
    }

    pub fn alloc_id(&mut self) -> NodeId {
        self.doc.alloc_id()
    }

    /// Insert a node under `parent` at `index` (None = top of the stack).
    pub fn insert(
        &mut self,
        mut node: Node,
        parent: Option<NodeId>,
        index: Option<usize>,
    ) -> NodeId {
        let id = node.id;
        self.touch(id);
        self.touch_parent_list(parent);
        node.parent = parent;
        self.doc.nodes.insert(id, node);
        if let Some(list) = self.doc.children_mut(parent) {
            let i = index.unwrap_or(list.len()).min(list.len());
            list.insert(i, id);
        }
        id
    }

    /// Insert or replace a node without touching any child list (caller wires the hierarchy).
    pub fn put(&mut self, node: Node) {
        self.touch(node.id);
        self.doc.nodes.insert(node.id, node);
    }

    /// Remove a node and its subtree.
    pub fn remove(&mut self, id: NodeId) {
        let Some(parent) = self.doc.get(id).map(|n| n.parent) else {
            return;
        };
        let removed = self.doc.subtree(id);
        let anchors: Vec<_> = self
            .doc
            .comments
            .iter()
            .enumerate()
            .filter(|(_, c)| removed.contains(&c.target))
            .map(|(i, c)| (i, c.position(self.doc)))
            .collect();
        for (i, at) in anchors {
            self.comments_mut()[i].fallback = at;
        }
        self.touch_parent_list(parent);
        if let Some(list) = self.doc.children_mut(parent) {
            list.retain(|c| *c != id);
        }
        for sid in self.doc.subtree(id) {
            self.touch(sid);
            self.doc.nodes.remove(&sid);
        }
    }

    /// Move a node to a new parent/index while keeping its world position.
    pub fn reparent(&mut self, id: NodeId, new_parent: Option<NodeId>, index: Option<usize>) {
        let Some(node) = self.doc.get(id) else { return };
        let old_parent = node.parent;
        if new_parent == Some(id) || new_parent.is_some_and(|p| self.doc.is_ancestor(id, p)) {
            return;
        }
        let world = self.doc.world_rect(id).unwrap();
        self.touch_parent_list(old_parent);
        if let Some(list) = self.doc.children_mut(old_parent) {
            list.retain(|c| *c != id);
        }
        self.touch_parent_list(new_parent);
        let origin = self.doc.parent_origin(new_parent);
        if let Some(list) = self.doc.children_mut(new_parent) {
            let i = index.unwrap_or(list.len()).min(list.len());
            list.insert(i, id);
        }
        let n = self.node_mut(id).unwrap();
        n.parent = new_parent;
        n.rect = world.translate(V2::ZERO - origin);
    }

    /// Reorder within the current parent.
    pub fn move_to_index(&mut self, id: NodeId, index: usize) {
        let Some(parent) = self.doc.get(id).map(|n| n.parent) else {
            return;
        };
        self.touch_parent_list(parent);
        if let Some(list) = self.doc.children_mut(parent) {
            list.retain(|c| *c != id);
            let i = index.min(list.len());
            list.insert(i, id);
        }
    }
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    /// Bumps on every commit, undo and redo, so savers can tell whether anything changed.
    generation: u64,
}

impl History {
    /// Finalize a recording into a change. Returns false when nothing changed.
    pub fn commit(&mut self, doc: &Document, rec: Recording, coalesce_key: Option<String>) -> bool {
        let entries: Vec<Entry> = rec
            .order
            .iter()
            .map(|id| Entry {
                id: *id,
                before: rec.before[id].clone(),
                after: doc.nodes.get(id).cloned(),
            })
            .filter(|e| e.before != e.after)
            .collect();
        let root = rec
            .root_before
            .filter(|b| *b != doc.root)
            .map(|b| (b, doc.root.clone()));
        let comments = rec
            .comments_before
            .filter(|b| *b != doc.comments)
            .map(|b| (b, doc.comments.clone()));
        if entries.is_empty() && root.is_none() && comments.is_none() {
            return false;
        }
        let change = Change {
            label: rec.label,
            entries,
            root,
            next_id: (rec.next_id_before, doc.next_id),
            coalesce_key,
            comments,
        };
        self.generation += 1;
        if let (Some(key), Some(top)) = (&change.coalesce_key, self.undo.last_mut())
            && self.redo.is_empty()
            && top.coalesce_key.as_ref() == Some(key)
        {
            merge(top, change);
            return true;
        }
        self.redo.clear();
        self.undo.push(change);
        if self.undo.len() > MAX_DEPTH {
            self.undo.remove(0);
        }
        true
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        let c = self.undo.pop()?;
        c.apply(doc, false);
        self.generation += 1;
        let label = c.label.clone();
        self.redo.push(c);
        Some(label)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        let c = self.redo.pop()?;
        c.apply(doc, true);
        self.generation += 1;
        let label = c.label.clone();
        self.undo.push(c);
        Some(label)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn undo_labels(&self) -> impl Iterator<Item = &str> {
        self.undo.iter().map(|c| c.label.as_str())
    }
    /// Stop coalescing into the current top entry (e.g. on focus change).
    pub fn seal(&mut self) {
        if let Some(top) = self.undo.last_mut() {
            top.coalesce_key = None;
        }
    }
}

/// Fold `newer` into `older`, keeping the oldest before-state of each node.
fn merge(older: &mut Change, newer: Change) {
    for e in newer.entries {
        if let Some(existing) = older.entries.iter_mut().find(|x| x.id == e.id) {
            existing.after = e.after;
        } else {
            older.entries.push(e);
        }
    }
    older.entries.retain(|e| e.before != e.after);
    match (&mut older.root, newer.root) {
        (Some((_, after)), Some((_, new_after))) => *after = new_after,
        (r @ None, Some(nr)) => *r = Some(nr),
        _ => {}
    }
    match (&mut older.comments, newer.comments) {
        (Some((_, after)), Some((_, new_after))) => *after = new_after,
        (c @ None, Some(nc)) => *c = Some(nc),
        _ => {}
    }
    older.next_id.1 = newer.next_id.1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::NodeKind;

    fn edit(doc: &mut Document, h: &mut History, key: Option<&str>, f: impl FnOnce(&mut Tx)) {
        let mut rec = Recording::new("edit", doc);
        f(&mut Tx::new(doc, &mut rec));
        h.commit(doc, rec, key.map(String::from));
    }

    fn add(tx: &mut Tx, kind: NodeKind, r: Rect, parent: Option<NodeId>) -> NodeId {
        let id = tx.alloc_id();
        tx.insert(Node::new(id, kind, r), parent, None)
    }

    #[test]
    fn undo_redo_insert_move_remove() {
        let mut doc = Document::default();
        let mut h = History::default();
        let mut id = NodeId(0);
        edit(&mut doc, &mut h, None, |tx| {
            id = add(tx, NodeKind::Rect, Rect::new(0., 0., 10., 10.), None)
        });
        let s1 = doc.clone();
        edit(&mut doc, &mut h, None, |tx| {
            tx.node_mut(id).unwrap().rect.x = 50.0
        });
        let s2 = doc.clone();
        edit(&mut doc, &mut h, None, |tx| tx.remove(id));
        assert!(doc.nodes.is_empty());
        doc.validate().unwrap();

        h.undo(&mut doc);
        assert_eq!(doc, s2);
        h.undo(&mut doc);
        assert_eq!(doc, s1);
        h.undo(&mut doc);
        assert_eq!(doc, Document::default());
        assert!(h.undo(&mut doc).is_none());
        h.redo(&mut doc);
        h.redo(&mut doc);
        assert_eq!(doc, s2);
        doc.validate().unwrap();
    }

    #[test]
    fn noop_transaction_not_recorded() {
        let mut doc = Document::default();
        let mut h = History::default();
        edit(&mut doc, &mut h, None, |tx| {
            add(tx, NodeKind::Rect, Rect::new(0., 0., 1., 1.), None);
        });
        edit(&mut doc, &mut h, None, |tx| {
            let n = tx.node_mut(NodeId(1)).unwrap();
            n.rect.x += 5.0;
            n.rect.x -= 5.0;
        });
        assert_eq!(h.undo_len(), 1);
    }

    #[test]
    fn coalescing_merges_and_new_edit_clears_redo() {
        let mut doc = Document::default();
        let mut h = History::default();
        edit(&mut doc, &mut h, None, |tx| {
            add(tx, NodeKind::Rect, Rect::new(0., 0., 1., 1.), None);
        });
        for i in 1..=5 {
            edit(&mut doc, &mut h, Some("opacity"), |tx| {
                tx.node_mut(NodeId(1)).unwrap().style.opacity = 1.0 - i as f64 * 0.1
            });
        }
        assert_eq!(h.undo_len(), 2);
        h.undo(&mut doc);
        assert_eq!(doc.get(NodeId(1)).unwrap().style.opacity, 1.0);
        assert!(h.can_redo());
        edit(&mut doc, &mut h, None, |tx| {
            tx.node_mut(NodeId(1)).unwrap().rect.y = 3.0
        });
        assert!(!h.can_redo());
    }

    #[test]
    fn reparent_keeps_world_position_and_undoes() {
        let mut doc = Document::default();
        let mut h = History::default();
        let (mut f, mut r) = (NodeId(0), NodeId(0));
        edit(&mut doc, &mut h, None, |tx| {
            f = add(
                tx,
                NodeKind::Frame { clip: true },
                Rect::new(100., 100., 300., 300.),
                None,
            );
            r = add(tx, NodeKind::Rect, Rect::new(150., 160., 10., 10.), None);
        });
        let before = doc.clone();
        edit(&mut doc, &mut h, None, |tx| tx.reparent(r, Some(f), None));
        assert_eq!(doc.get(r).unwrap().rect.min(), crate::geom::v2(50., 60.));
        assert_eq!(
            doc.world_rect(r).unwrap().min(),
            crate::geom::v2(150., 160.)
        );
        assert_eq!(doc.root, vec![f]);
        doc.validate().unwrap();
        // Can't parent a frame into its own child.
        edit(&mut doc, &mut h, None, |tx| tx.reparent(f, Some(r), None));
        doc.validate().unwrap();
        h.undo(&mut doc);
        assert_eq!(doc, before);
    }

    #[test]
    fn remove_frame_removes_subtree() {
        let mut doc = Document::default();
        let mut h = History::default();
        edit(&mut doc, &mut h, None, |tx| {
            let f = add(
                tx,
                NodeKind::Frame { clip: true },
                Rect::new(0., 0., 100., 100.),
                None,
            );
            add(tx, NodeKind::Ellipse, Rect::new(1., 1., 5., 5.), Some(f));
        });
        let full = doc.clone();
        edit(&mut doc, &mut h, None, |tx| tx.remove(NodeId(1)));
        assert!(doc.nodes.is_empty() && doc.root.is_empty());
        h.undo(&mut doc);
        assert_eq!(doc, full);
    }
}
