//! Shared comment operations for the native UI and MCP.
use super::Editor;
use crate::geom::{V2, v2};
use crate::model::{Comment, NodeId};
use crate::project::{new_id, now_ms};

fn body(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 10_000 {
        return Err("comment must contain 1–10000 characters".into());
    }
    Ok(text.to_string())
}
impl Editor {
    pub fn add_comment(&mut self, target: NodeId, at: V2, text: &str) -> Result<String, String> {
        let text = body(text)?;
        let n = self
            .doc
            .get(target)
            .ok_or("comment target does not exist")?;
        let r = self.doc.world_rect(target).unwrap();
        if !at.x.is_finite() || !at.y.is_finite() || !r.contains(at) {
            return Err("comment point must be inside the target's bounds".into());
        }
        let id = new_id();
        let c = Comment {
            id: id.clone(),
            target,
            relative: v2(
                if r.w > 0.0 {
                    ((at.x - r.x) / r.w).clamp(0.0, 1.0)
                } else {
                    0.5
                },
                if r.h > 0.0 {
                    ((at.y - r.y) / r.h).clamp(0.0, 1.0)
                } else {
                    0.5
                },
            ),
            fallback: at,
            target_name: n.name.clone(),
            body: text,
            created_at: now_ms(),
            updated_at: now_ms(),
            resolved: false,
        };
        self.edit("Add comment", |tx| tx.comments_mut().push(c));
        Ok(id)
    }
    pub fn update_comment(&mut self, id: &str, text: &str) -> Result<(), String> {
        let text = body(text)?;
        let i = self.comment_index(id)?;
        self.edit("Edit comment", |tx| {
            let c = &mut tx.comments_mut()[i];
            c.body = text;
            c.updated_at = now_ms();
        });
        Ok(())
    }
    pub fn resolve_comment(&mut self, id: &str, resolved: bool) -> Result<(), String> {
        let i = self.comment_index(id)?;
        self.edit("Comment status", |tx| {
            let c = &mut tx.comments_mut()[i];
            c.resolved = resolved;
            c.updated_at = now_ms();
        });
        Ok(())
    }
    pub fn delete_comment(&mut self, id: &str) -> Result<(), String> {
        let i = self.comment_index(id)?;
        self.edit("Delete comment", |tx| {
            tx.comments_mut().remove(i);
        });
        Ok(())
    }
    fn comment_index(&self, id: &str) -> Result<usize, String> {
        self.doc
            .comments
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| "comment does not exist".into())
    }
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub target: NodeId,
    pub at: V2,
    pub text: String,
    pub editing: Option<String>,
    pub focus: bool,
}
impl Editor {
    pub fn start_comment(&mut self, target: NodeId, at: V2) {
        if self.comment_draft.is_some() {
            self.status = "Post or cancel your draft before choosing another item".into();
            return;
        }
        let Some(r) = self.doc.world_rect(target) else {
            return;
        };
        let at = v2(at.x.clamp(r.x, r.right()), at.y.clamp(r.y, r.bottom()));
        self.comment_draft = Some(Draft {
            target,
            at,
            text: String::new(),
            editing: None,
            focus: true,
        });
        self.focused_comment = None;
        self.revision += 1;
    }
    pub fn post_comment(&mut self) -> Result<String, String> {
        let d = self.comment_draft.clone().ok_or("no comment draft")?;
        if self.doc.get(d.target).is_none() {
            return Err("The referenced element was removed. Cancel this draft.".into());
        }
        let id = if let Some(id) = d.editing {
            self.update_comment(&id, &d.text)?;
            id
        } else {
            self.add_comment(d.target, d.at, &d.text)?
        };
        self.comment_draft = None;
        self.focused_comment = Some(id.clone());
        self.status = "Comment saved".into();
        Ok(id)
    }
    pub fn focus_comment(&mut self, id: &str) {
        if self.comment_draft.is_some() {
            self.status = "Post or cancel your draft before opening another comment".into();
            return;
        }
        let page =
            (0..self.pages.len()).find(|p| self.page_doc(*p).comments.iter().any(|c| c.id == id));
        if let Some(page) = page {
            self.switch_page(page);
            self.focused_comment = Some(id.into());
            if let Some(c) = self.doc.comments.iter().find(|c| c.id == id) {
                let r = self.doc.world_rect(c.target).unwrap_or_else(|| {
                    crate::geom::Rect::new(c.fallback.x, c.fallback.y, 1.0, 1.0)
                });
                self.camera.fit(&r.inflate(80.0), self.viewport, 60.0, 1.0);
            }
            self.run(super::shortcuts::Action::Tool(super::Tool::Comment));
            self.revision += 1;
        }
    }
}
