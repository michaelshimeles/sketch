//! Feedback anchored to a design element, separate from the exported scene.
use super::{Document, NodeId};
use crate::geom::{V2, v2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub target: NodeId,
    pub relative: V2,
    pub fallback: V2,
    pub target_name: String,
    pub body: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub resolved: bool,
}
impl Comment {
    pub fn position(&self, doc: &Document) -> V2 {
        doc.world_rect(self.target).map_or(self.fallback, |r| {
            v2(r.x + r.w * self.relative.x, r.y + r.h * self.relative.y)
        })
    }
}
