//! Multiple pages per project. The active page lives in `Editor::{doc, history, camera}`;
//! the others are parked in their [`PageSlot`] with their own history and camera.

use super::{Editor, Gesture};
use crate::geom::Camera;
use crate::history::History;
use crate::model::{Document, NodeId};
use crate::project::{PageData, new_id};

#[derive(Debug)]
pub struct PageStash {
    pub doc: Document,
    pub history: History,
    pub camera: Camera,
    pub selection: Vec<NodeId>,
    /// The camera was never set (fit to content when first shown).
    pub needs_fit: bool,
}

#[derive(Debug)]
pub struct PageSlot {
    pub id: String,
    pub name: String,
    /// Parked state for inactive pages; `None` for the active one.
    pub stash: Option<Box<PageStash>>,
    /// History generation at the last save.
    pub saved_gen: u64,
}

impl PageSlot {
    pub fn new(name: &str) -> PageSlot {
        PageSlot {
            id: new_id(),
            name: name.into(),
            stash: None,
            saved_gen: 0,
        }
    }
}

impl Editor {
    /// Replace the content with a project's pages, keeping tool settings and the clipboard
    /// (so copy in one project and paste in another works).
    pub fn open_pages(&mut self, pages: Vec<PageData>, active: Option<&str>) {
        assert!(!pages.is_empty(), "a project has at least one page");
        let active_i = active
            .and_then(|a| pages.iter().position(|p| p.id == a))
            .unwrap_or(0);
        let clipboard = std::mem::take(&mut self.clipboard);
        let clipboard_roots = std::mem::take(&mut self.clipboard_roots);
        let clipboard_page = self.clipboard_page.take();
        let clipboard_world = std::mem::take(&mut self.clipboard_world);
        let first = pages[active_i].clone();
        let doc = Document {
            name: first.name.clone(),
            ..first.doc
        };
        self.load(doc, first.camera, None);
        self.clipboard = clipboard;
        self.clipboard_roots = clipboard_roots;
        self.clipboard_page = clipboard_page;
        self.clipboard_world = clipboard_world;
        self.pages = pages
            .into_iter()
            .enumerate()
            .map(|(i, p)| PageSlot {
                id: p.id,
                name: p.name,
                stash: (i != active_i).then(|| {
                    Box::new(PageStash {
                        needs_fit: p.camera.is_none(),
                        camera: p.camera.unwrap_or_default(),
                        doc: p.doc,
                        history: History::default(),
                        selection: Vec::new(),
                    })
                }),
                saved_gen: 0,
            })
            .collect();
        self.active_page = active_i;
        self.meta_dirty = false;
    }

    pub fn active_page_id(&self) -> &str {
        &self.pages[self.active_page].id
    }

    pub fn page_name(&self) -> &str {
        &self.pages[self.active_page].name
    }

    /// Make page `i` active, parking the current one.
    pub fn switch_page(&mut self, i: usize) {
        if self.comment_draft.is_some() {
            self.status = "Post or cancel your comment before changing pages".into();
            return;
        }
        if i == self.active_page || i >= self.pages.len() {
            return;
        }
        self.cancel_gesture();
        self.end_text_edit();
        self.commit();
        self.gesture = Gesture::None;
        let parked = PageStash {
            doc: std::mem::take(&mut self.doc),
            history: std::mem::take(&mut self.history),
            camera: self.camera,
            selection: std::mem::take(&mut self.selection),
            needs_fit: false,
        };
        self.pages[self.active_page].stash = Some(Box::new(parked));
        let s = self.pages[i]
            .stash
            .take()
            .expect("inactive page has a stash");
        self.doc = s.doc;
        self.history = s.history;
        self.camera = s.camera;
        self.selection = s.selection;
        self.active_page = i;
        self.hover = None;
        self.guides.clear();
        self.marquee = None;
        if s.needs_fit {
            self.zoom_to_fit();
            self.pending_fit = true;
        }
        self.meta_dirty = true; // the active page is remembered in the manifest
        self.revision += 1;
        self.doc_stamp = super::next_stamp();
    }

    fn unique_page_name(&self, base: &str) -> String {
        let mut n = base.to_string();
        let mut i = 2;
        while self.pages.iter().any(|p| p.name == n) {
            n = format!("{base} {i}");
            i += 1;
        }
        n
    }

    /// Add an empty page after the active one and switch to it.
    pub fn add_page(&mut self) -> usize {
        let at = self.active_page + 1;
        self.insert_empty_page(at, None);
        self.switch_page(at);
        self.status = format!("Added {}", self.pages[at].name);
        at
    }

    /// Append an empty page without leaving the current one (agents adding pages).
    pub fn add_page_in_background(&mut self, name: Option<&str>) -> usize {
        let at = self.pages.len();
        self.insert_empty_page(at, name);
        self.revision += 1;
        at
    }

    fn insert_empty_page(&mut self, at: usize, name: Option<&str>) {
        let base = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("Page {}", self.pages.len() + 1));
        let name = self.unique_page_name(&base);
        let mut slot = PageSlot::new(&name);
        slot.stash = Some(Box::new(PageStash {
            doc: Document {
                name: name.clone(),
                ..Default::default()
            },
            history: History::default(),
            camera: Camera::default(),
            selection: Vec::new(),
            needs_fit: true,
        }));
        // Never saved: force the first autosave.
        slot.saved_gen = u64::MAX;
        self.pages.insert(at, slot);
        if self.active_page >= at {
            self.active_page += 1;
        }
        self.meta_dirty = true;
    }

    pub fn duplicate_page(&mut self, i: usize) -> usize {
        if i >= self.pages.len() {
            return self.active_page;
        }
        self.commit();
        let mut src = self.page_data(i);
        for c in &mut src.doc.comments {
            c.id = new_id();
        }
        let name = self.unique_page_name(&format!("{} copy", src.name));
        let mut slot = PageSlot::new(&name);
        slot.saved_gen = u64::MAX;
        slot.stash = Some(Box::new(PageStash {
            doc: Document {
                name: name.clone(),
                ..src.doc
            },
            history: History::default(),
            camera: src.camera.unwrap_or_default(),
            selection: Vec::new(),
            needs_fit: src.camera.is_none(),
        }));
        self.pages.insert(i + 1, slot);
        if self.active_page > i {
            self.active_page += 1;
        }
        self.meta_dirty = true;
        self.switch_page(i + 1);
        i + 1
    }

    pub fn rename_page(&mut self, i: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() || i >= self.pages.len() || self.pages[i].name == name {
            return;
        }
        self.pages[i].name = name.to_string();
        if i == self.active_page {
            self.doc.name = name.to_string();
            self.doc_stamp = super::next_stamp();
        } else if let Some(s) = &mut self.pages[i].stash {
            s.doc.name = name.to_string();
        }
        self.meta_dirty = true;
        self.revision += 1;
    }

    /// Delete page `i` (never the last one). Returns the removed page id for file cleanup.
    pub fn delete_page(&mut self, i: usize) -> Option<String> {
        if self.comment_draft.is_some() {
            return None;
        }
        if self.pages.len() <= 1 || i >= self.pages.len() {
            return None;
        }
        if i == self.active_page {
            self.switch_page(if i + 1 < self.pages.len() {
                i + 1
            } else {
                i - 1
            });
        }
        let removed = self.pages.remove(i);
        if self.active_page > i {
            self.active_page -= 1;
        }
        self.meta_dirty = true;
        self.revision += 1;
        self.status = format!("Deleted {}", removed.name);
        Some(removed.id)
    }

    /// Move page `from` to position `to`.
    pub fn move_page(&mut self, from: usize, to: usize) {
        if from >= self.pages.len() || to >= self.pages.len() || from == to {
            return;
        }
        let active_id = self.active_page_id().to_string();
        let p = self.pages.remove(from);
        self.pages.insert(to, p);
        self.active_page = self.pages.iter().position(|p| p.id == active_id).unwrap();
        self.meta_dirty = true;
        self.revision += 1;
    }

    fn page_gen(&self, i: usize) -> u64 {
        if i == self.active_page {
            self.history.generation()
        } else {
            self.pages[i]
                .stash
                .as_ref()
                .map_or(0, |s| s.history.generation())
        }
    }

    /// Pages whose content changed since their last save.
    pub fn dirty_pages(&self) -> Vec<usize> {
        (0..self.pages.len())
            .filter(|i| self.page_gen(*i) != self.pages[*i].saved_gen)
            .collect()
    }

    /// Anything (content, page list, names) needs saving.
    pub fn project_dirty(&self) -> bool {
        self.meta_dirty || !self.dirty_pages().is_empty()
    }

    pub fn mark_page_saved(&mut self, i: usize) {
        self.pages[i].saved_gen = self.page_gen(i);
        if i == self.active_page {
            self.mark_saved();
        }
    }

    /// Snapshot of page `i` for saving or export.
    pub fn page_data(&self, i: usize) -> PageData {
        let slot = &self.pages[i];
        let (doc, camera) = if i == self.active_page {
            (self.doc.clone(), Some(self.camera))
        } else {
            let s = slot.stash.as_ref().expect("inactive page has a stash");
            (s.doc.clone(), (!s.needs_fit).then_some(s.camera))
        };
        PageData {
            id: slot.id.clone(),
            name: slot.name.clone(),
            doc: Document {
                name: slot.name.clone(),
                ..doc
            },
            camera,
        }
    }

    /// Run `f` with page `i` in the active slot (its document, undo history and selection)
    /// without switching the page the user sees. Edits made by `f` land in page `i`'s own
    /// history; the user's view, camera and dirty marker are left alone.
    pub fn with_page<R>(&mut self, i: usize, f: impl FnOnce(&mut Editor) -> R) -> R {
        if i == self.active_page || i >= self.pages.len() {
            return f(self);
        }
        self.commit();
        let mut s = self.pages[i]
            .stash
            .take()
            .expect("inactive page has a stash");
        let swap = |ed: &mut Editor, s: &mut PageStash| {
            std::mem::swap(&mut ed.doc, &mut s.doc);
            std::mem::swap(&mut ed.history, &mut s.history);
            std::mem::swap(&mut ed.selection, &mut s.selection);
        };
        let saved_marker = self.saved_marker;
        swap(self, &mut s);
        let r = f(self);
        self.commit();
        swap(self, &mut s);
        self.saved_marker = saved_marker;
        self.pages[i].stash = Some(s);
        self.revision += 1;
        self.doc_stamp = super::next_stamp();
        r
    }

    /// The document of page `i` (active or parked).
    pub fn page_doc(&self, i: usize) -> &Document {
        if i == self.active_page {
            &self.doc
        } else {
            &self.pages[i].stash.as_ref().unwrap().doc
        }
    }
}
