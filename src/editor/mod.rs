//! The editor: document + history + camera + selection + tool state machine.
//!
//! It consumes platform-agnostic [`InputEvent`]s, so every interaction can be scripted in tests.

pub mod commands;
pub mod comments;
pub mod hit;
pub mod input;
pub mod pages;
pub mod shortcuts;
pub mod snap;
pub mod structure;
pub mod text_edit;
pub mod vector_ops;
pub mod whiteboard;

use crate::geom::{Camera, Rect, V2, next_zoom_step, v2};
use crate::history::{History, Recording, Tx};
use crate::ink::{self, InkBuilder};
use crate::model::{Color, Document, Node, NodeId, NodeKind, StrokePoint, TextAlign};
use crate::text;
pub use input::{Button, InputEvent, Key, Modifiers};
pub use pages::{PageSlot, PageStash};
use snap::Guide;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Hand,
    Frame,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Diamond,
    Line,
    Arrow,
    Pen,
    Text,
    Sticky,
    Pencil,
    Eraser,
    Laser,
    Comment,
}

impl Tool {
    pub const ALL: [Tool; 17] = [
        Tool::Select,
        Tool::Hand,
        Tool::Frame,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Diamond,
        Tool::Polygon,
        Tool::Star,
        Tool::Line,
        Tool::Arrow,
        Tool::Pen,
        Tool::Text,
        Tool::Sticky,
        Tool::Pencil,
        Tool::Eraser,
        Tool::Laser,
        Tool::Comment,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Hand => "Hand",
            Tool::Frame => "Frame",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygon",
            Tool::Star => "Star",
            Tool::Diamond => "Diamond",
            Tool::Line => "Line",
            Tool::Arrow => "Arrow",
            Tool::Pen => "Pen",
            Tool::Text => "Text",
            Tool::Sticky => "Sticky note",
            Tool::Pencil => "Pencil",
            Tool::Eraser => "Eraser",
            Tool::Laser => "Laser pointer",
            Tool::Comment => "Comment",
        }
    }
    pub fn shortcut(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Hand => "H",
            Tool::Frame => "F",
            Tool::Rect => "R",
            Tool::Ellipse => "O",
            Tool::Polygon => "⇧Y",
            Tool::Star => "⇧S",
            Tool::Diamond => "D",
            Tool::Line => "L",
            Tool::Arrow => "⇧L",
            Tool::Pen => "P",
            Tool::Text => "T",
            Tool::Sticky => "S",
            Tool::Pencil => "⇧P",
            Tool::Eraser => "E",
            Tool::Laser => "K",
            Tool::Comment => "C",
        }
    }
}

/// Requests the editor can't fulfil itself (dialogs, file system, clipboard).
#[derive(Debug, Clone, PartialEq)]
pub enum AppRequest {
    /// Export with these settings (opens a save dialog).
    Export(crate::io::export::Spec),
    /// Export using the settings in the inspector, in this format.
    ExportFormat(crate::io::export::Format),
    /// Render the selection and put the PNG on the clipboard.
    CopyAsPng,
    /// Back to the projects home screen.
    Home,
    New,
    Open,
    Save,
    SaveAs,
    /// Pick an image file and place it on the canvas.
    PlaceImage,
    /// Pick an image for fill layer `.1` of these layers.
    ImageFill(Vec<NodeId>, usize),
}

#[derive(Debug, Clone)]
pub struct TextEdit {
    pub id: NodeId,
    /// Caret as a char index.
    pub caret: usize,
    /// Other end of the selection (None = no selection).
    pub anchor: Option<usize>,
}

#[derive(Debug)]
enum Gesture {
    None,
    Pan {
        last: V2,
    },
    Marquee {
        start: V2,
        scope: Option<NodeId>,
        base: Vec<NodeId>,
    },
    /// Pressed on a node; becomes Move once the pointer travels far enough.
    Press {
        start_screen: V2,
        start: V2,
        click_select: Option<NodeId>,
    },
    Move {
        start: V2,
        origins: Vec<(NodeId, Rect)>,
        bounds0: Rect,
    },
    Resize {
        handle: (f64, f64),
        start: V2,
        bounds0: Rect,
        origins: Vec<(NodeId, Rect)>,
        /// Every node in the resized subtrees as it was at drag start (constraints and
        /// group scaling are recomputed from this each move, so nothing drifts).
        snapshot: Vec<Node>,
    },
    Create {
        tool: Tool,
        start: V2,
        id: NodeId,
    },
    Draw {
        builder: Box<InkBuilder>,
        id: NodeId,
    },
    Erase,
    /// Pen tool: dragging out the handles of the point just placed.
    Pen,
    /// Drag-selecting text inside the text being edited.
    TextSelect {
        id: NodeId,
    },
    /// Dragging an arrow end (new arrows too); `start` is the press position.
    ConnectorEnd {
        id: NodeId,
        end: usize,
        start: V2,
    },
    /// Bending an arrow by its middle point.
    ConnectorMid {
        id: NodeId,
    },
    Laser,
    /// Dragging a point or handle of the path being edited.
    PathPart {
        si: usize,
        pi: usize,
        part: vector_ops::PointPart,
    },
}

/// Drag distance (screen px) before a press turns into a move.
const DRAG_THRESHOLD: f64 = 3.0;
const PASTE_OFFSET: f64 = 16.0;

pub struct Editor {
    pub doc: Document,
    pub history: History,
    pub camera: Camera,
    /// Canvas viewport size in logical pixels.
    pub viewport: V2,
    pub selection: Vec<NodeId>,
    pub tool: Tool,
    pub mods: Modifiers,
    pub space_down: bool,
    pub text_edit: Option<TextEdit>,
    pub guides: Vec<Guide>,
    /// World-space marquee rectangle while dragging.
    pub marquee: Option<Rect>,
    pub hover: Option<NodeId>,
    pub pointer: V2,
    pub ink_width: f64,
    pub ink_smoothing: f64,
    pub ink_color: Color,
    pub snapping: bool,
    pub file_path: Option<PathBuf>,
    pub requests: Vec<AppRequest>,
    pub status: String,
    clipboard: Vec<Node>,
    clipboard_roots: Vec<NodeId>,
    /// Page the clipboard was copied from; pasting elsewhere keeps positions.
    clipboard_page: Option<String>,
    /// World rects of the clipboard roots at copy time.
    clipboard_world: Vec<Rect>,
    gesture: Gesture,
    rec: Option<(Recording, Option<String>)>,
    saved_marker: Option<usize>,
    last_click: Option<(V2, f64)>,
    /// Bumps on every document or view change; drives repaints.
    pub revision: u64,
    /// Process-unique stamp of the document content: changes whenever the document may
    /// have changed (edits, undo, page switch, load). Renderers reuse work while it holds.
    pub doc_stamp: u64,
    /// Fit the camera to the content once the real viewport size is known.
    pub pending_fit: bool,
    /// Pages of the open project; the active one's content is `doc`/`history`/`camera`.
    pub pages: Vec<PageSlot>,
    pub active_page: usize,
    /// Page list or names changed (not tracked by undo history).
    pub meta_dirty: bool,
    /// Auto-layout frames not re-laid out while a child is being dragged inside them.
    pub layout_suspended: Vec<NodeId>,
    /// Pen tool in progress.
    pub pen: Option<vector_ops::PenState>,
    /// Path point editing in progress.
    pub path_edit: Option<vector_ops::PathEdit>,
    /// Alt-hover distance measurements (world space).
    pub measures: Vec<Guide>,
    /// Shape an arrow end would bind to (highlighted while dragging).
    pub bind_hover: Option<NodeId>,
    /// Laser pointer trail: world points with the time they were added (ms).
    pub laser: Vec<(V2, f64)>,
    /// Latest input time (ms), for animations.
    pub now_ms: f64,
    /// Route for new arrows.
    pub arrow_route: crate::model::Route,
    /// Hand-drawn style for new shapes (None = clean lines).
    pub sketch: Option<crate::model::rough::Rough>,
    pub sticky_color: Color,
    pub comment_draft: Option<comments::Draft>,
    pub focused_comment: Option<String>,
    /// Requests for the chrome (command palette, rename...).
    pub ui_events: Vec<commands::UiEvent>,
    /// System clipboard (set by the app; None in tests unless injected).
    pub clipboard_io: Option<Box<dyn crate::io::clipboard::Provider>>,
    /// Text we last put on the system clipboard (to recognize our own copy).
    clipboard_written: Option<String>,
    clipboard_style: Option<(crate::model::Style, crate::model::TextStyle)>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new(Document::default())
    }
}

impl Editor {
    pub fn new(doc: Document) -> Editor {
        let name = match doc.name.as_str() {
            "" | "Untitled" => "Page 1".to_string(),
            n => n.to_string(),
        };
        Editor {
            doc,
            history: History::default(),
            camera: Camera::default(),
            viewport: v2(1280.0, 800.0),
            selection: Vec::new(),
            tool: Tool::Select,
            mods: Modifiers::NONE,
            space_down: false,
            text_edit: None,
            guides: Vec::new(),
            marquee: None,
            hover: None,
            pointer: V2::ZERO,
            ink_width: 3.0,
            ink_smoothing: 0.5,
            ink_color: Color::INK,
            snapping: true,
            file_path: None,
            requests: Vec::new(),
            status: String::new(),
            clipboard: Vec::new(),
            clipboard_roots: Vec::new(),
            clipboard_page: None,
            clipboard_world: Vec::new(),
            gesture: Gesture::None,
            rec: None,
            saved_marker: Some(0),
            last_click: None,
            revision: 0,
            doc_stamp: next_stamp(),
            pending_fit: false,
            pages: vec![PageSlot::new(&name)],
            active_page: 0,
            meta_dirty: false,
            layout_suspended: Vec::new(),
            pen: None,
            path_edit: None,
            measures: Vec::new(),
            bind_hover: None,
            laser: Vec::new(),
            now_ms: 0.0,
            arrow_route: crate::model::Route::Straight,
            sketch: None,
            sticky_color: Color::rgb(0xff, 0xdf, 0x6b),
            comment_draft: None,
            focused_comment: None,
            ui_events: Vec::new(),
            clipboard_style: None,
            clipboard_io: None,
            clipboard_written: None,
        }
    }

    // ------------------------------------------------------------------ transactions

    /// Open a transaction (committing any open one first).
    pub fn begin(&mut self, label: &str) {
        self.begin_coalescing(label, None);
    }

    pub fn begin_coalescing(&mut self, label: &str, key: Option<&str>) {
        self.commit();
        self.rec = Some((Recording::new(label, &self.doc), key.map(String::from)));
    }

    /// Mutable access to the document inside the open transaction (opens one if needed).
    pub fn tx(&mut self) -> Tx<'_> {
        if self.rec.is_none() {
            self.rec = Some((Recording::new("Edit", &self.doc), None));
        }
        self.revision += 1;
        self.doc_stamp = next_stamp();
        let (rec, _) = self.rec.as_mut().unwrap();
        Tx::new(&mut self.doc, rec)
    }

    pub fn commit(&mut self) {
        self.fixup();
        if let Some((rec, key)) = self.rec.take()
            && self.history.commit(&self.doc, rec, key)
        {
            self.revision += 1;
        }
    }

    /// Run one edit as a complete transaction.
    pub fn edit<R>(&mut self, label: &str, f: impl FnOnce(&mut Tx) -> R) -> R {
        self.begin(label);
        let r = f(&mut self.tx());
        self.commit();
        r
    }

    pub fn edit_coalescing<R>(
        &mut self,
        label: &str,
        key: &str,
        f: impl FnOnce(&mut Tx) -> R,
    ) -> R {
        self.begin_coalescing(label, Some(key));
        let r = f(&mut self.tx());
        self.commit();
        r
    }

    pub fn undo(&mut self) {
        self.cancel_gesture();
        self.end_text_edit();
        self.commit();
        if let Some(l) = self.history.undo(&mut self.doc) {
            self.status = format!("Undo {l}");
        }
        self.after_history();
    }

    pub fn redo(&mut self) {
        self.cancel_gesture();
        self.end_text_edit();
        self.commit();
        if let Some(l) = self.history.redo(&mut self.doc) {
            self.status = format!("Redo {l}");
        }
        self.after_history();
    }

    fn after_history(&mut self) {
        self.revision += 1;
        self.doc_stamp = next_stamp();
        self.selection.retain(|id| self.doc.nodes.contains_key(id));
    }

    /// A pointer gesture (drag, draw, resize...) is in progress.
    pub fn is_busy(&self) -> bool {
        !matches!(self.gesture, Gesture::None)
    }

    /// Commit an open transaction unless a gesture or text edit still owns it.
    pub fn commit_pending(&mut self) {
        if !self.is_busy() && self.text_edit.is_none() {
            self.commit();
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_marker != Some(self.history.undo_len()) || self.rec.is_some()
    }

    pub fn mark_saved(&mut self) {
        self.history.seal();
        self.saved_marker = Some(self.history.undo_len());
    }

    /// Replace the document (open/new).
    pub fn load(&mut self, doc: Document, camera: Option<Camera>, path: Option<PathBuf>) {
        *self = Editor {
            viewport: self.viewport,
            ink_width: self.ink_width,
            ink_smoothing: self.ink_smoothing,
            ink_color: self.ink_color,
            snapping: self.snapping,
            clipboard: std::mem::take(&mut self.clipboard),
            clipboard_roots: std::mem::take(&mut self.clipboard_roots),
            clipboard_page: self.clipboard_page.take(),
            clipboard_world: std::mem::take(&mut self.clipboard_world),
            sketch: self.sketch,
            arrow_route: self.arrow_route,
            sticky_color: self.sticky_color,
            clipboard_io: self.clipboard_io.take(),
            clipboard_written: self.clipboard_written.take(),
            ..Editor::new(doc)
        };
        self.file_path = path;
        match camera {
            Some(c) => self.camera = c,
            None => {
                self.zoom_to_fit();
                self.pending_fit = true;
            }
        }
        self.revision += 1;
    }

    // ------------------------------------------------------------------ queries

    pub fn screen_to_world(&self, p: V2) -> V2 {
        self.camera.screen_to_world(p)
    }

    /// Selected nodes without those whose ancestor is also selected.
    pub fn top_selection(&self) -> Vec<NodeId> {
        self.selection
            .iter()
            .copied()
            .filter(|id| self.doc.nodes.contains_key(id))
            .filter(|id| {
                !self
                    .doc
                    .ancestors(*id)
                    .iter()
                    .any(|a| self.selection.contains(a))
            })
            .collect()
    }

    pub fn selection_bounds(&self) -> Option<Rect> {
        let rects: Vec<Rect> = self
            .top_selection()
            .iter()
            .filter_map(|id| self.doc.world_rect(*id))
            .collect();
        Rect::union_all(&rects)
    }

    pub fn select(&mut self, ids: Vec<NodeId>) {
        if self
            .text_edit
            .as_ref()
            .is_some_and(|t| !ids.contains(&t.id))
        {
            self.end_text_edit();
        }
        self.selection = ids;
    }

    fn snap_threshold(&self) -> f64 {
        snap::SNAP_PX / self.camera.zoom
    }

    fn snapping_active(&self) -> bool {
        self.snapping && !self.mods.command()
    }

    /// Rects of nodes that moving `ids` may snap against: siblings, the parent frame, top-level frames.
    fn snap_candidates(&self, ids: &[NodeId]) -> Vec<Rect> {
        let excluded: Vec<NodeId> = ids.iter().flat_map(|id| self.doc.subtree(*id)).collect();
        let vis = self
            .camera
            .visible_world(self.viewport)
            .inflate(200.0 / self.camera.zoom);
        let mut parents: Vec<Option<NodeId>> = ids
            .iter()
            .map(|id| self.doc.get(*id).and_then(|n| n.parent))
            .collect();
        parents.dedup();
        let mut out = Vec::new();
        let mut push = |id: NodeId| {
            if !excluded.contains(&id)
                && self.doc.is_effectively_visible(id)
                && let Some(r) = self.doc.world_rect(id).filter(|r| r.intersects(&vis))
            {
                out.push(r);
            }
        };
        for p in parents.iter().copied() {
            for c in self.doc.children(p) {
                push(*c);
            }
            if let Some(p) = p {
                push(p);
            }
        }
        for id in self.doc.root.clone() {
            if self.doc.get(id).is_some_and(|n| n.is_frame()) {
                push(id);
            }
        }
        out.dedup();
        out
    }

    /// Topmost frame (deepest first) containing world point `p`, excluding `exclude` subtrees.
    pub fn frame_at(&self, p: V2, exclude: &[NodeId]) -> Option<NodeId> {
        let excluded: Vec<NodeId> = exclude
            .iter()
            .flat_map(|id| self.doc.subtree(*id))
            .collect();
        self.doc.paint_order().into_iter().rev().find(|id| {
            !excluded.contains(id)
                && self.doc.get(*id).is_some_and(|n| n.is_frame())
                && self.doc.is_effectively_visible(*id)
                && self.doc.world_rect(*id).is_some_and(|r| r.contains(p))
                && hit::inside_ancestor_clips(&self.doc, *id, p)
        })
    }

    /// Screen-space frame label rectangles for top-level frames (also used by the renderer).
    pub fn frame_labels(&self) -> Vec<(NodeId, Rect)> {
        self.doc
            .root
            .iter()
            .filter_map(|id| {
                let n = self.doc.get(*id)?;
                if !n.is_frame() || !n.visible {
                    return None;
                }
                let sr = self.camera.world_rect_to_screen(&self.doc.world_rect(*id)?);
                let w = text::layout(&n.name, 12.0, None, TextAlign::Left).width;
                Some((*id, hit::frame_label_rect(&sr, w.min(sr.w.max(24.0)))))
            })
            .collect()
    }

    fn label_at(&self, p: V2) -> Option<NodeId> {
        self.frame_labels()
            .into_iter()
            .rev()
            .find(|(id, r)| r.contains(p) && !self.doc.is_effectively_locked(*id))
            .map(|(id, _)| id)
    }

    // ------------------------------------------------------------------ input

    pub fn handle(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::Modifiers(m) => {
                self.mods = m;
                self.update_measures();
            }
            InputEvent::PointerDown {
                pos,
                button,
                pressure,
                time_ms,
            } => {
                self.pointer = pos;
                self.pointer_down(pos, button, pressure, time_ms)
            }
            InputEvent::PointerMove {
                pos,
                pressure,
                time_ms,
            } => {
                self.pointer = pos;
                self.pointer_move(pos, pressure, time_ms)
            }
            InputEvent::PointerUp {
                pos,
                button,
                time_ms,
            } => {
                self.pointer = pos;
                self.pointer_up(pos, button, time_ms)
            }
            InputEvent::Scroll { delta, pos } => {
                if self.mods.ctrl || self.mods.logo {
                    let factor = (-delta.y * 0.01).exp();
                    self.camera.zoom_about(pos, self.camera.zoom * factor);
                } else {
                    self.camera.pan_by_screen(-delta);
                }
                self.revision += 1;
            }
            InputEvent::Zoom { factor, pos } => {
                self.camera.zoom_about(pos, self.camera.zoom * factor);
                self.revision += 1;
            }
            InputEvent::KeyDown { key, repeat } => self.key_down(key, repeat),
            InputEvent::KeyUp { key } => {
                if key == Key::Space {
                    self.space_down = false;
                }
            }
            InputEvent::Text(s) => {
                if self.text_edit.is_some() && !self.mods.command() {
                    for c in s.chars().filter(|c| !c.is_control()) {
                        self.insert_text(&c.to_string());
                    }
                }
            }
        }
    }

    fn pointer_down(&mut self, pos: V2, button: Button, pressure: Option<f64>, time_ms: f64) {
        let world = self.screen_to_world(pos);
        if button == Button::Middle
            || (button == Button::Primary && (self.space_down || self.tool == Tool::Hand))
        {
            self.gesture = Gesture::Pan { last: pos };
            return;
        }
        if button != Button::Primary {
            return;
        }
        let double = self
            .last_click
            .is_some_and(|(p, t)| p.dist(pos) < 5.0 && time_ms - t < 400.0);
        self.last_click = Some((pos, time_ms));

        match self.tool {
            Tool::Comment => {
                if let Some(target) = hit::comment_hit_test(&self.doc, world, self.camera.zoom) {
                    self.start_comment(target, world);
                }
            }
            Tool::Select | Tool::Hand => self.select_down(pos, world, double),
            Tool::Pencil => {
                self.end_text_edit();
                self.begin("Draw");
                let zoom = self.camera.zoom;
                let mut builder = InkBuilder::new(self.ink_smoothing, 0.75 / zoom);
                builder.push(world, pressure, time_ms, zoom);
                let parent = self.frame_at(world, &[]);
                let width = self.ink_width / zoom;
                let color = self.ink_color;
                let id = {
                    let mut tx = self.tx();
                    let id = tx.alloc_id();
                    let mut n = Node::new(
                        id,
                        NodeKind::Stroke {
                            points: Vec::new(),
                            width,
                            thinning: 0.6,
                        },
                        Rect::default(),
                    );
                    n.style.set_fill(Some(color));
                    tx.insert(n, parent, None)
                };
                self.update_stroke_node(id, builder.points().to_vec());
                self.gesture = Gesture::Draw {
                    builder: Box::new(builder),
                    id,
                };
            }
            Tool::Eraser => {
                self.end_text_edit();
                self.begin("Erase");
                self.gesture = Gesture::Erase;
                self.erase_at(world);
            }
            Tool::Text => {
                if let Some(id) = hit::hit_test(&self.doc, world, self.camera.zoom)
                    .filter(|id| self.doc.get(*id).unwrap().kind.text().is_some())
                {
                    self.start_text_edit(id, Some(world));
                    return;
                }
                self.end_text_edit();
                self.begin("Add text");
                let parent = self.frame_at(world, &[]);
                let origin = self.doc.parent_origin(parent);
                let size = (16.0 / self.camera.zoom.max(0.25)).round().max(1.0);
                let lh = text::line_height(size);
                let p = (world - origin - v2(0.0, lh * 0.5)).round();
                let id = {
                    let mut tx = self.tx();
                    let id = tx.alloc_id();
                    let n = Node::new(
                        id,
                        NodeKind::Text {
                            text: String::new(),
                            font_size: size,
                            auto_width: true,
                            align: TextAlign::Left,
                        },
                        Rect::new(p.x, p.y, 1.0, lh),
                    );
                    tx.insert(n, parent, None)
                };
                self.selection = vec![id];
                self.text_edit = Some(TextEdit {
                    id,
                    caret: 0,
                    anchor: None,
                });
                self.tool = Tool::Select;
            }
            Tool::Pen => {
                self.pen_down(world, pos);
                self.gesture = Gesture::Pen;
            }
            Tool::Arrow => self.arrow_down(if self.snapping { world.round() } else { world }),
            Tool::Sticky => self.sticky_down(world),
            Tool::Laser => {
                self.laser_point(world, time_ms);
                self.gesture = Gesture::Laser;
            }
            Tool::Frame
            | Tool::Rect
            | Tool::Ellipse
            | Tool::Diamond
            | Tool::Polygon
            | Tool::Star
            | Tool::Line => {
                self.end_text_edit();
                let label = format!("Add {}", self.tool.label().to_lowercase());
                self.begin(&label);
                let start = if self.snapping { world.round() } else { world };
                let parent = self.frame_at(start, &[]);
                let origin = self.doc.parent_origin(parent);
                let kind = match self.tool {
                    Tool::Frame => NodeKind::Frame { clip: true },
                    Tool::Rect => NodeKind::Rect,
                    Tool::Ellipse => NodeKind::Ellipse,
                    Tool::Polygon => NodeKind::Polygon { sides: 3 },
                    Tool::Diamond => NodeKind::Polygon { sides: 4 },
                    Tool::Star => NodeKind::Star {
                        points: 5,
                        inner: 0.38,
                    },
                    _ => NodeKind::Line {
                        a: V2::ZERO,
                        b: V2::ZERO,
                    },
                };
                let frame_count = self.doc.nodes.values().filter(|n| n.is_frame()).count();
                let sketch = self.sketch;
                let id = {
                    let mut tx = self.tx();
                    let id = tx.alloc_id();
                    let mut n = Node::new(
                        id,
                        kind,
                        Rect::new(start.x - origin.x, start.y - origin.y, 0.0, 0.0),
                    );
                    if n.is_frame() {
                        n.name = format!("Frame {}", frame_count + 1);
                    } else if let Some(r) = sketch {
                        n.style.rough = Some(crate::model::rough::Rough {
                            seed: (id.0 as u32).wrapping_mul(2654435761) | 1,
                            ..r
                        });
                    }
                    if matches!(n.kind, NodeKind::Polygon { sides: 4 }) {
                        n.name = "Diamond".into();
                    }
                    tx.insert(n, parent, None)
                };
                self.selection = vec![id];
                self.gesture = Gesture::Create {
                    tool: self.tool,
                    start,
                    id,
                };
            }
        }
    }

    fn select_down(&mut self, pos: V2, world: V2, double: bool) {
        if self.path_edit.is_none() && self.text_edit.is_none() {
            if let Some(&(dir, _)) = self.quick_knobs().iter().find(|(_, p)| p.dist(pos) <= 9.0) {
                self.quick_connect(dir);
                return;
            }
            if self.connector_handle_down(pos, world) {
                return;
            }
        }
        if self.path_edit.is_some() {
            if let Some((si, pi, part)) = self.path_part_at(pos) {
                if double && part == vector_ops::PointPart::Anchor {
                    self.toggle_point_smooth(si, pi);
                    return;
                }
                if let Some(pe) = &mut self.path_edit {
                    pe.selected = Some((si, pi));
                }
                self.begin("Edit path");
                self.gesture = Gesture::PathPart { si, pi, part };
                return;
            }
            let on_path = self
                .path_edit
                .as_ref()
                .is_some_and(|pe| hit::node_contains(&self.doc, pe.id, world, self.camera.zoom));
            if !on_path {
                self.path_edit = None;
            } else {
                if let Some(pe) = &mut self.path_edit {
                    pe.selected = None;
                }
                return;
            }
        }
        // Resize handles of the current selection take priority.
        if self.text_edit.is_none()
            && let Some(b) = self.selection_bounds()
        {
            let bs = self.camera.world_rect_to_screen(&b);
            if let Some(handle) = hit::handle_at(&bs, pos) {
                let origins = self.top_selection_world();
                if !origins.is_empty() {
                    self.begin("Resize");
                    let snapshot = origins
                        .iter()
                        .flat_map(|(id, _)| self.doc.subtree(*id))
                        .filter_map(|id| self.doc.get(id).cloned())
                        .collect();
                    self.gesture = Gesture::Resize {
                        handle,
                        start: world,
                        bounds0: b,
                        origins,
                        snapshot,
                    };
                    return;
                }
            }
        }
        if let Some(id) = self.label_at(pos) {
            if !self.selection.contains(&id) {
                self.select(if self.mods.shift {
                    [self.selection.clone(), vec![id]].concat()
                } else {
                    vec![id]
                });
            }
            self.gesture = Gesture::Press {
                start_screen: pos,
                start: world,
                click_select: None,
            };
            return;
        }
        let hit = hit::hit_test(&self.doc, world, self.camera.zoom);
        // Editing text: clicks inside the text move the caret.
        if let (Some(te), Some(h)) = (&self.text_edit, hit)
            && te.id == h
        {
            self.start_text_edit(h, Some(world));
            if double {
                self.gesture = Gesture::None;
                self.select_word();
            }
            return;
        }
        self.end_text_edit();
        let Some(mut target) = hit else {
            self.start_marquee(world, None);
            return;
        };
        // Top-level frames with content act as canvas regions: their body starts a marquee.
        let node = self.doc.get(target).unwrap();
        if node.is_frame()
            && node.parent.is_none()
            && !node.children.is_empty()
            && !self.selection.contains(&target)
        {
            self.start_marquee(world, Some(target));
            return;
        }
        if double && node.kind.text().is_some() {
            self.start_text_edit(target, Some(world));
            return;
        }
        if double && matches!(node.kind, NodeKind::Path { .. }) && self.selection == [target] {
            self.start_path_edit(target);
            return;
        }
        // Groups and instances capture clicks until entered (double-click or Cmd-click).
        if !self.mods.command() {
            target = self.click_target(target, double);
        }
        // If an ancestor is already selected, keep dragging the ancestor.
        if !double
            && let Some(a) = self
                .doc
                .ancestors(target)
                .into_iter()
                .find(|a| self.selection.contains(a))
        {
            target = a;
        }
        let click_select = if self.mods.shift {
            if self.selection.contains(&target) {
                Some(target) // toggle off on release if no drag
            } else {
                self.selection.push(target);
                None
            }
        } else if self.selection.contains(&target) {
            // Pressing an already-selected node keeps a multi-selection for dragging,
            // and narrows to it on a click without drag.
            (self.selection.len() > 1).then_some(target)
        } else {
            self.select(vec![target]);
            None
        };
        self.gesture = Gesture::Press {
            start_screen: pos,
            start: world,
            click_select,
        };
    }

    /// Alt held with a selection: measure to the hovered layer, or to the parent frame.
    pub fn update_measures(&mut self) {
        self.measures.clear();
        if !self.mods.alt || self.tool != Tool::Select {
            return;
        }
        let Some(sel) = self.selection_bounds() else {
            return;
        };
        let other = self
            .hover
            .filter(|h| !self.selection.contains(h))
            .or_else(|| {
                self.selection
                    .first()
                    .and_then(|id| self.doc.get(*id)?.parent)
            });
        if let Some(r) = other.and_then(|o| self.doc.world_rect(o)) {
            self.measures = snap::measure(&sel, &r);
        }
    }

    /// The layer a click on `hit` selects: the outermost group or instance not yet entered.
    fn click_target(&self, hit: NodeId, double: bool) -> NodeId {
        let mut chain = self.doc.ancestors(hit);
        chain.reverse();
        chain.push(hit);
        for a in chain {
            let n = self.doc.get(a).unwrap();
            let captures = n.is_group() || self.is_instance_root(a);
            if !captures || a == hit {
                continue;
            }
            let entered = self
                .selection
                .iter()
                .any(|s| *s != a && self.doc.is_ancestor(a, *s));
            let selected = self.selection.contains(&a);
            if entered || (selected && double) {
                continue;
            }
            return a;
        }
        hit
    }

    fn start_marquee(&mut self, world: V2, scope: Option<NodeId>) {
        let base = if self.mods.shift {
            self.selection.clone()
        } else {
            Vec::new()
        };
        if !self.mods.shift {
            self.select(Vec::new());
        }
        self.gesture = Gesture::Marquee {
            start: world,
            scope,
            base,
        };
    }

    fn top_selection_world(&self) -> Vec<(NodeId, Rect)> {
        self.top_selection()
            .into_iter()
            .filter(|id| !self.doc.is_effectively_locked(*id))
            .filter_map(|id| Some((id, self.doc.world_rect(id)?)))
            .collect()
    }

    fn pointer_move(&mut self, pos: V2, pressure: Option<f64>, time_ms: f64) {
        let world = self.screen_to_world(pos);
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::None => {
                self.hover = if self.tool == Tool::Comment {
                    hit::comment_hit_test(&self.doc, world, self.camera.zoom)
                } else if self.tool == Tool::Select {
                    self.label_at(pos)
                        .or_else(|| hit::hit_test(&self.doc, world, self.camera.zoom))
                } else {
                    None
                };
                self.update_measures();
            }
            Gesture::Pan { last } => {
                self.camera.pan_by_screen(pos - last);
                self.revision += 1;
                self.gesture = Gesture::Pan { last: pos };
            }
            Gesture::Press {
                start_screen,
                start,
                click_select,
            } => {
                if pos.dist(start_screen) < DRAG_THRESHOLD {
                    self.gesture = Gesture::Press {
                        start_screen,
                        start,
                        click_select,
                    };
                    return;
                }
                let mut origins = self.top_selection_world();
                if origins.is_empty() {
                    return;
                }
                let bounds0 = Rect::union_all(origins.iter().map(|(_, r)| r)).unwrap();
                self.begin(if self.mods.alt { "Duplicate" } else { "Move" });
                if self.mods.alt {
                    // Alt-drag drags a copy, leaving the original in place (one undo step).
                    let roots: Vec<NodeId> = origins.iter().map(|(id, _)| *id).collect();
                    let world: Vec<Rect> = origins.iter().map(|(_, r)| *r).collect();
                    let nodes: Vec<Node> = roots
                        .iter()
                        .flat_map(|r| self.doc.subtree(*r))
                        .filter_map(|id| self.doc.get(id).cloned())
                        .collect();
                    let new =
                        insert_subtrees_tx(&mut self.tx(), &nodes, &roots, &world, V2::ZERO, true);
                    origins = new.iter().copied().zip(world).collect();
                    self.selection = new;
                }
                // Auto-layout parents hold still while their children are dragged around.
                self.layout_suspended = origins
                    .iter()
                    .filter_map(|(id, _)| {
                        let n = self.doc.get(*id)?;
                        let p = self.doc.get(n.parent?)?;
                        (p.layout.is_some() && !n.layout_child.absolute).then_some(p.id)
                    })
                    .collect();
                self.gesture = Gesture::Move {
                    start,
                    origins,
                    bounds0,
                };
                self.pointer_move(pos, pressure, time_ms);
            }
            Gesture::Move {
                start,
                origins,
                bounds0,
            } => {
                let mut d = world - start;
                if self.mods.shift {
                    // Constrain to the dominant axis.
                    if d.x.abs() > d.y.abs() {
                        d.y = 0.0
                    } else {
                        d.x = 0.0
                    }
                }
                let mut moved = bounds0.translate(d);
                self.guides.clear();
                if self.snapping_active() {
                    let ids: Vec<NodeId> = origins.iter().map(|(id, _)| *id).collect();
                    let cands = self.snap_candidates(&ids);
                    let (sx, sy, guides) = snap::snap_rect(
                        &moved,
                        [true; 3],
                        [true; 3],
                        &cands,
                        self.snap_threshold(),
                    );
                    moved = moved.translate(v2(sx, sy));
                    self.guides = guides;
                    // Equal spacing between neighbors on axes that didn't edge-snap.
                    let (gx, gy, gaps) = snap::snap_spacing(&moved, &cands, self.snap_threshold());
                    let gx = if sx == 0.0 { gx } else { 0.0 };
                    let gy = if sy == 0.0 { gy } else { 0.0 };
                    if gx != 0.0 || gy != 0.0 || !gaps.is_empty() {
                        moved = moved.translate(v2(gx, gy));
                        self.guides.extend(gaps.into_iter().filter(|g| match g {
                            Guide::Gap { a, b } => {
                                ((a.y - b.y).abs() < 1e-9 && sx == 0.0)
                                    || ((a.x - b.x).abs() < 1e-9 && sy == 0.0)
                            }
                            _ => true,
                        }));
                    }
                    if sx == 0.0 && gx == 0.0 {
                        moved.x = moved.x.round();
                    }
                    if sy == 0.0 && gy == 0.0 {
                        moved.y = moved.y.round();
                    }
                }
                let d = moved.min() - bounds0.min();
                for (id, r0) in &origins {
                    let parent = self.doc.get(*id).and_then(|n| n.parent);
                    let o = self.doc.parent_origin(parent);
                    let r = r0.translate(d - o);
                    if let Some(n) = self.tx().node_mut(*id) {
                        n.rect = r;
                    }
                }
                let ids: Vec<NodeId> = origins.iter().map(|(id, _)| *id).collect();
                self.mark_overrides(&ids, "position");
                self.fixup();
                self.gesture = Gesture::Move {
                    start,
                    origins,
                    bounds0,
                };
            }
            Gesture::Resize {
                handle,
                start,
                bounds0,
                origins,
                snapshot,
            } => {
                let nb = self.resize_bounds(handle, start, world, bounds0);
                // Restore the drag-start state, then apply this frame's size from it.
                {
                    let mut tx = self.tx();
                    for n in &snapshot {
                        if tx.doc().get(n.id) != Some(n) {
                            tx.put(n.clone());
                        }
                    }
                }
                let ignore_constraints = self.mods.command();
                let sx = if bounds0.w > 1e-9 {
                    nb.w / bounds0.w
                } else {
                    1.0
                };
                let sy = if bounds0.h > 1e-9 {
                    nb.h / bounds0.h
                } else {
                    1.0
                };
                for (id, r0) in &origins {
                    let wr = Rect::new(
                        nb.x + (r0.x - bounds0.x) * sx,
                        nb.y + (r0.y - bounds0.y) * sy,
                        (r0.w * sx).max(0.0),
                        (r0.h * sy).max(0.0),
                    );
                    let parent = self.doc.get(*id).and_then(|n| n.parent);
                    let o = self.doc.parent_origin(parent);
                    let old = self.doc.get(*id).map(|n| n.rect).unwrap_or_default();
                    let new = wr.translate(V2::ZERO - o);
                    let mut tx = self.tx();
                    if let Some(n) = tx.node_mut(*id) {
                        n.set_rect_scaling(new);
                        // Manual resizing fixes the size (no more hug/fill) on that axis.
                        if (new.w - old.w).abs() > 1e-9 {
                            n.layout_child.sizing[0] = crate::model::Sizing::Fixed;
                        }
                        if (new.h - old.h).abs() > 1e-9 {
                            n.layout_child.sizing[1] = crate::model::Sizing::Fixed;
                        }
                    }
                    let group = tx.doc().get(*id).is_some_and(|n| n.is_group());
                    if group || !ignore_constraints {
                        crate::layout::resize_children(&mut tx, *id, old, new);
                    }
                    self.fit_text(*id);
                }
                let ids: Vec<NodeId> = origins.iter().map(|(id, _)| *id).collect();
                self.mark_overrides(&ids, "size");
                self.fixup();
                self.gesture = Gesture::Resize {
                    handle,
                    start,
                    bounds0,
                    origins,
                    snapshot,
                };
            }
            Gesture::Marquee { start, scope, base } => {
                let r = Rect::from_points(start, world);
                self.marquee = Some(r);
                let pool: Vec<NodeId> = self.doc.children(scope).to_vec();
                let mut sel = base.clone();
                for id in pool {
                    let n = self.doc.get(id).unwrap();
                    if !n.visible || n.locked || sel.contains(&id) {
                        continue;
                    }
                    let Some(wr) = self.doc.world_rect(id) else {
                        continue;
                    };
                    // Top-level frames must be fully enclosed; everything else just intersect.
                    let hit = if n.is_frame() && scope.is_none() {
                        r.contains_rect(&wr)
                    } else {
                        r.intersects(&wr)
                    };
                    if hit {
                        sel.push(id);
                    }
                }
                self.selection = sel;
                self.gesture = Gesture::Marquee { start, scope, base };
            }
            Gesture::Create { tool, start, id } => {
                let mut end = if self.snapping { world.round() } else { world };
                if self.mods.shift && tool != Tool::Line {
                    let d = end - start;
                    let s = d.x.abs().max(d.y.abs());
                    end = start + v2(s * d.x.signum(), s * d.y.signum());
                }
                if self.mods.shift && tool == Tool::Line {
                    // Snap the angle to 45° increments.
                    let d = end - start;
                    let a = (d.y.atan2(d.x) / std::f64::consts::FRAC_PI_4).round()
                        * std::f64::consts::FRAC_PI_4;
                    end = start + v2(a.cos(), a.sin()) * d.len();
                }
                let (a, b) = if self.mods.alt {
                    (start - (end - start), end)
                } else {
                    (start, end)
                };
                let wr = Rect::from_points(a, b);
                let parent = self.doc.get(id).and_then(|n| n.parent);
                let o = self.doc.parent_origin(parent);
                if let Some(n) = self.tx().node_mut(id) {
                    n.rect = wr.translate(V2::ZERO - o);
                    if let NodeKind::Line { a: la, b: lb } = &mut n.kind {
                        *la = a - wr.min();
                        *lb = b - wr.min();
                    }
                }
                self.gesture = Gesture::Create { tool, start, id };
            }
            Gesture::Draw { mut builder, id } => {
                builder.push(world, pressure, time_ms, self.camera.zoom);
                self.update_stroke_node(id, builder.points().to_vec());
                self.gesture = Gesture::Draw { builder, id };
            }
            Gesture::Erase => {
                self.erase_at(world);
                self.gesture = Gesture::Erase;
            }
            Gesture::Pen => {
                self.pen_drag(world);
                self.gesture = Gesture::Pen;
            }
            Gesture::ConnectorEnd { id, end, start } => {
                let w = if self.snapping && !self.mods.command() {
                    world.round()
                } else {
                    world
                };
                self.drag_connector_end(id, end, w);
                self.gesture = Gesture::ConnectorEnd { id, end, start };
            }
            Gesture::ConnectorMid { id } => {
                self.drag_connector_mid(id, world);
                self.gesture = Gesture::ConnectorMid { id };
            }
            Gesture::Laser => {
                self.laser_point(world, time_ms);
                self.gesture = Gesture::Laser;
            }
            Gesture::TextSelect { id } => {
                if let (Some(n), Some(wr)) = (self.doc.get(id), self.doc.world_rect(id))
                    && let Some(l) = text::layout_node(n)
                    && let Some(te) = &mut self.text_edit
                {
                    te.caret = l.hit(world - wr.min() - text::text_offset(n));
                }
                self.gesture = Gesture::TextSelect { id };
            }
            Gesture::PathPart { si, pi, part } => {
                let w = if self.snapping && !self.mods.command() {
                    world.round()
                } else {
                    world
                };
                self.drag_path_part(si, pi, part, w);
                self.gesture = Gesture::PathPart { si, pi, part };
            }
        }
    }

    fn resize_bounds(&self, (hx, hy): (f64, f64), start: V2, world: V2, b0: Rect) -> Rect {
        let d = world - start;
        let (mut x0, mut y0, mut x1, mut y1) = (b0.x, b0.y, b0.right(), b0.bottom());
        let sym = self.mods.alt;
        if hx == 0.0 {
            x0 += d.x;
            if sym {
                x1 -= d.x;
            }
        } else if hx == 1.0 {
            x1 += d.x;
            if sym {
                x0 -= d.x;
            }
        }
        if hy == 0.0 {
            y0 += d.y;
            if sym {
                y1 -= d.y;
            }
        } else if hy == 1.0 {
            y1 += d.y;
            if sym {
                y0 -= d.y;
            }
        }
        if self.mods.shift && b0.w > 0.0 && b0.h > 0.0 {
            let aspect = b0.w / b0.h;
            let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
            let edge_x = hy == 0.5;
            let edge_y = hx == 0.5;
            let (nw, nh) = if edge_x || (!edge_y && w / aspect >= h) {
                (w, w / aspect)
            } else {
                (h * aspect, h)
            };
            // Keep the anchor (opposite side) fixed.
            if hx == 0.0 {
                x0 = x1 - nw
            } else if hx == 1.0 {
                x1 = x0 + nw
            } else {
                let c = (x0 + x1) * 0.5;
                x0 = c - nw * 0.5;
                x1 = c + nw * 0.5
            }
            if hy == 0.0 {
                y0 = y1 - nh
            } else if hy == 1.0 {
                y1 = y0 + nh
            } else {
                let c = (y0 + y1) * 0.5;
                y0 = c - nh * 0.5;
                y1 = c + nh * 0.5
            }
        }
        let mut r = Rect::from_points(v2(x0, y0), v2(x1, y1));
        if self.snapping_active() && !self.mods.shift {
            let ids: Vec<NodeId> = self.top_selection();
            let cands = self.snap_candidates(&ids);
            let ux = [hx == 0.0, false, hx == 1.0];
            let uy = [hy == 0.0, false, hy == 1.0];
            let (sx, sy, _) = snap::snap_rect(&r, ux, uy, &cands, self.snap_threshold());
            if hx == 0.0 {
                r.x += sx;
                r.w -= sx
            } else if hx == 1.0 {
                r.w += sx
            }
            if hy == 0.0 {
                r.y += sy;
                r.h -= sy
            } else if hy == 1.0 {
                r.h += sy
            }
            let round = |v: f64| v.round();
            if sx == 0.0 && hx != 0.5 {
                let (l, rr) = (round(r.x), round(r.right()));
                r.x = l;
                r.w = rr - l;
            }
            if sy == 0.0 && hy != 0.5 {
                let (t, bb) = (round(r.y), round(r.bottom()));
                r.y = t;
                r.h = bb - t;
            }
        }
        r.w = r.w.max(1.0 / self.camera.zoom);
        r.h = r.h.max(1.0 / self.camera.zoom);
        r
    }

    fn pointer_up(&mut self, pos: V2, button: Button, _time_ms: f64) {
        let world = self.screen_to_world(pos);
        let _ = button;
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::None | Gesture::Pan { .. } => {}
            Gesture::Press { click_select, .. } => {
                if let Some(id) = click_select {
                    if self.mods.shift {
                        self.selection.retain(|s| *s != id);
                    } else {
                        self.selection = vec![id];
                    }
                }
            }
            Gesture::Move { origins, .. } => {
                let ids: Vec<NodeId> = origins.iter().map(|(id, _)| *id).collect();
                self.unbind_moved_connectors(&ids);
                self.drop_into_auto_layout(&ids);
                self.layout_suspended.clear();
                self.reparent_after_move(ids);
                self.guides.clear();
                self.commit();
            }
            Gesture::Resize { .. } => {
                self.guides.clear();
                self.commit();
            }
            Gesture::Marquee { .. } => {
                self.marquee = None;
            }
            Gesture::Create { tool, start, id } => {
                let r = self.doc.get(id).map(|n| n.rect).unwrap_or_default();
                // A click without dragging creates a default-sized shape at the pointer.
                if r.w * self.camera.zoom < 2.0 && r.h * self.camera.zoom < 2.0 {
                    let (w, h) = match tool {
                        Tool::Frame => (400.0, 300.0),
                        Tool::Line => (100.0, 0.0),
                        _ => (100.0, 100.0),
                    };
                    let parent = self.doc.get(id).and_then(|n| n.parent);
                    let o = self.doc.parent_origin(parent);
                    if let Some(n) = self.tx().node_mut(id) {
                        n.rect = Rect::new(start.x - o.x, start.y - o.y, w, h);
                        if let NodeKind::Line { a, b } = &mut n.kind {
                            *a = V2::ZERO;
                            *b = v2(w, 0.0);
                        }
                    }
                }
                if tool == Tool::Frame {
                    self.adopt_into_new_frame(id);
                }
                self.commit();
                self.tool = Tool::Select;
            }
            Gesture::Draw { builder, id } => {
                let zoom = self.camera.zoom;
                let pts = builder.finish(Some(world), 1.5 / zoom);
                self.update_stroke_node(id, pts);
                self.commit();
            }
            Gesture::Erase => self.commit(),
            Gesture::Pen => self.pen_up(),
            Gesture::ConnectorEnd { id, start, .. } => self.connector_end_up(id, start, world),
            Gesture::ConnectorMid { .. } => self.commit(),
            Gesture::Laser => {}
            Gesture::TextSelect { .. } => {
                if let Some(te) = &mut self.text_edit
                    && te.anchor == Some(te.caret)
                {
                    te.anchor = None;
                }
            }
            Gesture::PathPart { .. } => self.commit(),
        }
    }

    /// When a frame is drawn around existing siblings, they move into it.
    fn adopt_into_new_frame(&mut self, frame: NodeId) {
        let Some(fr) = self.doc.world_rect(frame) else {
            return;
        };
        let parent = self.doc.get(frame).and_then(|n| n.parent);
        let inside: Vec<NodeId> = self
            .doc
            .children(parent)
            .iter()
            .copied()
            .filter(|id| *id != frame)
            .filter(|id| {
                self.doc
                    .world_rect(*id)
                    .is_some_and(|r| fr.contains_rect(&r))
            })
            .collect();
        let mut tx = self.tx();
        for id in inside {
            tx.reparent(id, Some(frame), None);
        }
    }

    /// A connector dragged on its own lets go of shapes that didn't move with it.
    fn unbind_moved_connectors(&mut self, ids: &[NodeId]) {
        let moved: std::collections::HashSet<NodeId> =
            ids.iter().flat_map(|i| self.doc.subtree(*i)).collect();
        let mut tx = self.tx();
        for id in ids {
            let Some(n) = tx.doc().get(*id) else { continue };
            let NodeKind::Connector { start, end, .. } = &n.kind else {
                continue;
            };
            let drop =
                |b: &Option<crate::model::Binding>| b.is_some_and(|b| !moved.contains(&b.node));
            let (ds, de) = (drop(start), drop(end));
            if ds || de {
                let n = tx.node_mut(*id).unwrap();
                if let NodeKind::Connector { start, end, .. } = &mut n.kind {
                    if ds {
                        *start = None;
                    }
                    if de {
                        *end = None;
                    }
                }
            }
        }
    }

    /// Dragged children of an auto-layout frame that are still over it take a new place in
    /// its flow (by the drop position); dragged layers dropped onto an auto-layout frame
    /// join its flow at the drop position.
    fn drop_into_auto_layout(&mut self, ids: &[NodeId]) {
        for id in ids.iter().copied() {
            let Some(r) = self.doc.world_rect(id) else {
                continue;
            };
            let target = self.frame_at(r.center(), ids);
            let Some(t) = target.filter(|t| self.doc.get(*t).is_some_and(|f| f.layout.is_some()))
            else {
                continue;
            };
            let fo = self.doc.world_rect(t).unwrap().min();
            let idx = crate::layout::drop_index(&self.doc, t, id, r.center() - fo);
            let mut tx = self.tx();
            if tx.doc().get(id).and_then(|n| n.parent) == Some(t) {
                tx.move_to_index(id, idx);
            } else {
                tx.reparent(id, Some(t), Some(idx));
            }
        }
    }

    fn reparent_after_move(&mut self, ids: Vec<NodeId>) {
        for id in ids.iter().copied() {
            let Some(r) = self.doc.world_rect(id) else {
                continue;
            };
            let current = self.doc.get(id).and_then(|n| n.parent);
            let target = self.frame_at(r.center(), &ids);
            // Only move out of the current parent if fully outside it; move into a frame on center.
            let stay = current.is_some_and(|p| {
                target.is_none_or(|t| t == p || self.doc.is_ancestor(t, p))
                    && self.doc.world_rect(p).is_some_and(|pr| pr.intersects(&r))
            });
            if stay || target == current {
                continue;
            }
            self.tx().reparent(id, target, None);
        }
    }

    fn update_stroke_node(&mut self, id: NodeId, world_pts: Vec<StrokePoint>) {
        let Some(NodeKind::Stroke {
            width, thinning, ..
        }) = self.doc.get(id).map(|n| n.kind.clone())
        else {
            return;
        };
        let parent = self.doc.get(id).and_then(|n| n.parent);
        let o = self.doc.parent_origin(parent);
        let b = ink::bounds(&world_pts, width, thinning);
        let local: Vec<StrokePoint> = world_pts
            .iter()
            .map(|p| StrokePoint {
                x: p.x - b.x,
                y: p.y - b.y,
                p: p.p,
            })
            .collect();
        if let Some(n) = self.tx().node_mut(id) {
            n.rect = b.translate(V2::ZERO - o);
            if let NodeKind::Stroke { points, .. } = &mut n.kind {
                *points = local;
            }
        }
    }

    fn erase_at(&mut self, world: V2) {
        let zoom = self.camera.zoom;
        let victims: Vec<NodeId> = self
            .doc
            .paint_order()
            .into_iter()
            .filter(|id| matches!(self.doc.get(*id).unwrap().kind, NodeKind::Stroke { .. }))
            .filter(|id| {
                self.doc.is_effectively_visible(*id) && !self.doc.is_effectively_locked(*id)
            })
            .filter(|id| hit::node_contains(&self.doc, *id, world, zoom / 2.0))
            .collect();
        for v in victims {
            self.tx().remove(v);
            self.selection.retain(|s| *s != v);
        }
    }

    // ------------------------------------------------------------------ text

    /// Recompute a text node's box from its content.
    pub fn fit_text(&mut self, id: NodeId) {
        let Some(n) = self.doc.get(id) else { return };
        let NodeKind::Text { auto_width, .. } = &n.kind else {
            return;
        };
        let Some(l) = text::layout_node(n) else {
            return;
        };
        let (w, h) = (
            if *auto_width {
                l.width.max(1.0)
            } else {
                n.rect.w
            },
            l.height,
        );
        let r = n.rect;
        if ((r.w - w).abs() > 1e-9 || (r.h - h).abs() > 1e-9)
            && let Some(n) = self.tx().node_mut(id)
        {
            n.rect.w = w;
            n.rect.h = h;
        }
    }

    pub fn start_text_edit(&mut self, id: NodeId, at_world: Option<V2>) {
        let Some(n) = self.doc.get(id) else { return };
        let Some(t) = n.kind.text() else {
            return;
        };
        let len = t.chars().count();
        let caret = match (at_world, text::layout_node(n)) {
            (Some(p), Some(l)) => {
                let local = p - self.doc.world_rect(id).unwrap().min() - text::text_offset(n);
                l.hit(local)
            }
            _ => len,
        };
        if let Some(te) = self.text_edit.as_mut().filter(|te| te.id == id) {
            // Clicking inside the text being edited: place the caret (Shift extends) and
            // start a drag selection.
            if self.mods.shift {
                te.anchor.get_or_insert(te.caret);
            } else {
                te.anchor = Some(caret);
            }
            te.caret = caret;
            self.gesture = Gesture::TextSelect { id };
            return;
        }
        self.end_text_edit();
        self.begin("Edit text");
        self.selection = vec![id];
        self.text_edit = Some(TextEdit {
            id,
            caret,
            anchor: if at_world.is_none() { Some(0) } else { None },
        });
    }

    /// Select the word around the caret (double-click while editing).
    pub fn select_word(&mut self) {
        let Some(te) = self.text_edit.clone() else {
            return;
        };
        let Some(text) = self.doc.get(te.id).and_then(|n| n.kind.text()) else {
            return;
        };
        let chars: Vec<char> = text.chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
        let mut a = te.caret.min(chars.len());
        let mut b = a;
        while a > 0 && is_word(chars[a - 1]) {
            a -= 1;
        }
        while b < chars.len() && is_word(chars[b]) {
            b += 1;
        }
        if let Some(t) = &mut self.text_edit {
            t.anchor = Some(a);
            t.caret = b;
        }
    }

    /// Finish text editing; empty text nodes are removed.
    pub fn end_text_edit(&mut self) {
        let Some(te) = self.text_edit.take() else {
            return;
        };
        if matches!(self.gesture, Gesture::TextSelect { .. }) {
            self.gesture = Gesture::None;
        }
        let empty = matches!(self.doc.get(te.id).map(|n| &n.kind), Some(NodeKind::Text { text, .. }) if text.trim().is_empty());
        if empty {
            self.tx().remove(te.id);
            self.selection.retain(|s| *s != te.id);
        }
        self.commit();
    }

    // ------------------------------------------------------------------ keyboard

    fn key_down(&mut self, key: Key, repeat: bool) {
        let cmd_char = self.mods.command() && matches!(key, Key::Char(_));
        let select_all = self.mods.command() && matches!(key, Key::Char('a' | 'A'));
        if self.text_edit.is_some() && (!cmd_char || select_all) {
            self.text_key(&key);
            return;
        }
        if self.pen.is_some() && matches!(key, Key::Enter | Key::Escape) {
            self.finish_pen();
            return;
        }
        if self.path_edit.is_some() {
            match key {
                Key::Enter | Key::Escape => {
                    self.path_edit = None;
                    return;
                }
                Key::Delete | Key::Backspace if self.delete_path_point() => return,
                _ => {}
            }
        }
        if let Some(action) = shortcuts::lookup(&key, self.mods) {
            if !repeat || action.repeatable() {
                self.run(action);
            }
            return;
        }
        if key == Key::Space && !repeat {
            self.space_down = true;
        }
    }

    pub fn run(&mut self, action: shortcuts::Action) {
        use shortcuts::Action as A;
        match action {
            A::Tool(t) => {
                if self.comment_draft.is_some() {
                    self.status = "Post or cancel your comment first".into();
                    return;
                }
                self.end_text_edit();
                self.finish_pen();
                self.path_edit = None;
                self.tool = t;
                if t == Tool::Comment {
                    self.selection.clear();
                }
            }
            A::Escape => {
                if self.comment_draft.take().is_some() {
                    self.revision += 1;
                    return;
                }
                self.focused_comment = None;
                if !matches!(self.gesture, Gesture::None) {
                    self.cancel_gesture();
                } else if self.tool != Tool::Select {
                    self.tool = Tool::Select;
                } else {
                    // Select parent, then nothing.
                    let parents: Vec<NodeId> = self
                        .selection
                        .iter()
                        .filter_map(|id| self.doc.get(*id).and_then(|n| n.parent))
                        .collect();
                    self.selection = if parents.is_empty() {
                        Vec::new()
                    } else {
                        dedup(parents)
                    };
                }
            }
            A::EditText => {
                if let [id] = self.selection[..] {
                    if matches!(
                        self.doc.get(id).map(|n| &n.kind),
                        Some(NodeKind::Path { .. })
                    ) {
                        self.start_path_edit(id);
                    } else if matches!(
                        self.doc.get(id).map(|n| &n.kind),
                        Some(NodeKind::Text { .. })
                    ) {
                        self.start_text_edit(id, None);
                    } else if self.doc.get(id).is_some_and(|n| !n.children.is_empty()) {
                        self.selection = self.doc.get(id).unwrap().children.clone();
                    }
                }
            }
            A::Delete => self.delete_selection(),
            A::Undo => self.undo(),
            A::Redo => self.redo(),
            A::SelectAll => {
                let scope = self
                    .selection
                    .first()
                    .and_then(|id| self.doc.get(*id))
                    .and_then(|n| n.parent);
                self.selection = self
                    .doc
                    .children(scope)
                    .iter()
                    .copied()
                    .filter(|id| self.doc.get(*id).unwrap().visible)
                    .collect();
            }
            A::Copy => self.copy(),
            A::Cut => {
                self.copy();
                self.delete_selection();
            }
            A::Paste => self.paste_from_clipboard(),
            A::Duplicate => {
                let saved = (
                    self.clipboard.clone(),
                    self.clipboard_roots.clone(),
                    self.clipboard_page.clone(),
                    self.clipboard_world.clone(),
                );
                self.copy_internal();
                self.paste(v2(PASTE_OFFSET, PASTE_OFFSET));
                (
                    self.clipboard,
                    self.clipboard_roots,
                    self.clipboard_page,
                    self.clipboard_world,
                ) = saved;
            }
            A::Forward => self.reorder(|i, _| i + 1),
            A::Backward => self.reorder(|i, _| i.saturating_sub(1)),
            A::ToFront => self.reorder(|_, len| len),
            A::ToBack => self.reorder(|_, _| 0),
            A::ZoomIn => self.zoom_step(true),
            A::ZoomOut => self.zoom_step(false),
            A::Zoom100 => {
                self.camera.zoom_about(self.viewport * 0.5, 1.0);
                self.revision += 1;
            }
            A::ZoomFit => self.zoom_to_fit(),
            A::ZoomSelection => {
                if let Some(b) = self.selection_bounds() {
                    self.camera.fit(&b, self.viewport, 64.0, 8.0);
                    self.revision += 1;
                }
            }
            A::ToggleVisible => {
                let ids = self.top_selection();
                let show = ids.iter().any(|id| !self.doc.get(*id).unwrap().visible);
                self.edit(if show { "Show" } else { "Hide" }, |tx| {
                    for id in ids {
                        tx.node_mut(id).unwrap().visible = show;
                    }
                });
            }
            A::ToggleLock => {
                let ids = self.top_selection();
                let lock = ids.iter().any(|id| !self.doc.get(*id).unwrap().locked);
                self.edit(if lock { "Lock" } else { "Unlock" }, |tx| {
                    for id in ids {
                        tx.node_mut(id).unwrap().locked = lock;
                    }
                });
                if lock {
                    self.selection.clear();
                }
            }
            A::Nudge(dx, dy) => {
                let ids: Vec<NodeId> = self
                    .top_selection()
                    .into_iter()
                    .filter(|id| !self.doc.is_effectively_locked(*id))
                    .collect();
                if !ids.is_empty() {
                    self.edit_coalescing("Nudge", "nudge", |tx| {
                        for id in ids {
                            let n = tx.node_mut(id).unwrap();
                            n.rect.x += dx;
                            n.rect.y += dy;
                        }
                    });
                }
            }
            A::WrapInFrame => self.wrap_in_frame(),
            A::New => self.requests.push(AppRequest::New),
            A::Open => self.requests.push(AppRequest::Open),
            A::Save => self.requests.push(AppRequest::Save),
            A::SaveAs => self.requests.push(AppRequest::SaveAs),
            A::ExportPng => self
                .requests
                .push(AppRequest::ExportFormat(crate::io::export::Format::Png)),
            A::ExportSvg => self
                .requests
                .push(AppRequest::ExportFormat(crate::io::export::Format::Svg)),
            A::ExportPdf => self
                .requests
                .push(AppRequest::ExportFormat(crate::io::export::Format::Pdf)),
            A::CopyAsPng => self.requests.push(AppRequest::CopyAsPng),
            A::CopyAsSvg => self.copy_as_svg(),
            A::ToggleSnapping => {
                self.snapping = !self.snapping;
                self.status = format!("Snapping {}", if self.snapping { "on" } else { "off" });
            }
            A::Group => self.group_selection(),
            A::Ungroup => self.ungroup_selection(),
            A::CreateComponent => self.create_component(),
            A::DetachInstance => self.detach_instance(),
            A::AddAutoLayout => self.add_auto_layout(),
            A::RemoveAutoLayout => self.remove_auto_layout(),
            A::Align(axis, to) => self.align(axis as usize, to),
            A::Distribute(axis) => self.distribute(axis as usize),
            A::Boolean(op) => self.boolean(op),
            A::Flatten => self.flatten_selection(),
            A::Bold => self.toggle_bold(),
            A::Italic => self.toggle_italic(),
            A::Underline => self.toggle_underline(),
            A::Strikethrough => self.toggle_strike(),
            A::PlaceImage => self.requests.push(AppRequest::PlaceImage),
            A::ToggleSketch => self.toggle_sketch(),
            A::CommandPalette => self.ui_events.push(commands::UiEvent::TogglePalette),
            A::Rename => {
                if let Some(id) = self.selection.first() {
                    self.ui_events.push(commands::UiEvent::Rename(*id));
                }
            }
            A::ToggleUi => self.ui_events.push(commands::UiEvent::ToggleUi),
            A::ToggleHelp => self.ui_events.push(commands::UiEvent::ToggleHelp),
            A::ToggleGrid => self.ui_events.push(commands::UiEvent::ToggleGrid),
            A::SelectNext => self.select_sibling(true),
            A::SelectPrev => self.select_sibling(false),
            A::SelectParent => self.select_parent(),
            A::NextPage => {
                let i = (self.active_page + 1) % self.pages.len();
                self.switch_page(i);
            }
            A::PrevPage => {
                let i = (self.active_page + self.pages.len() - 1) % self.pages.len();
                self.switch_page(i);
            }
            A::FlipH => self.flip(0),
            A::FlipV => self.flip(1),
            A::CopyProps => self.copy_props(),
            A::PasteProps => self.paste_props(),
            A::Home => self.requests.push(AppRequest::Home),
        }
    }

    fn cancel_gesture(&mut self) {
        match std::mem::replace(&mut self.gesture, Gesture::None) {
            Gesture::Move { .. }
            | Gesture::Resize { .. }
            | Gesture::Create { .. }
            | Gesture::Draw { .. }
            | Gesture::PathPart { .. }
            | Gesture::ConnectorEnd { .. }
            | Gesture::ConnectorMid { .. }
            | Gesture::Erase => {
                // Roll back the open transaction.
                if let Some((rec, key)) = self.rec.take() {
                    let mut h = History::default();
                    if h.commit(&self.doc, rec, key) {
                        h.undo(&mut self.doc);
                    }
                    self.after_history();
                }
            }
            _ => {}
        }
        self.guides.clear();
        self.marquee = None;
        self.layout_suspended.clear();
        self.bind_hover = None;
    }

    pub fn zoom_step(&mut self, up: bool) {
        let z = next_zoom_step(self.camera.zoom, up);
        self.camera.zoom_about(self.viewport * 0.5, z);
        self.revision += 1;
    }

    pub fn zoom_to_fit(&mut self) {
        match self.doc.content_bounds() {
            Some(b) => self.camera.fit(&b, self.viewport, 64.0, 1.0),
            None => {
                self.camera = Camera {
                    pan: v2(-self.viewport.x * 0.5, -self.viewport.y * 0.5),
                    zoom: 1.0,
                }
            }
        }
        self.revision += 1;
    }

    pub fn delete_selection(&mut self) {
        let ids: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| !self.doc.is_effectively_locked(*id))
            .collect();
        if ids.is_empty() {
            return;
        }
        self.end_text_edit();
        self.edit("Delete", |tx| {
            for id in ids {
                tx.remove(id);
            }
        });
        self.selection.clear();
    }

    /// Copy to the internal clipboard and, when available, the system clipboard.
    fn copy(&mut self) {
        self.copy_internal();
        if self.clipboard_roots.is_empty() {
            return;
        }
        if let Some(io) = &mut self.clipboard_io {
            let text = crate::io::clipboard::encode(
                &self.clipboard,
                &self.clipboard_roots,
                &self.clipboard_world,
                self.clipboard_page.as_deref(),
            );
            io.write_text(&text);
            self.clipboard_written = Some(text);
        }
    }

    /// Paste from the system clipboard when it holds something newer than our own copy
    /// (layers from another window, Excalidraw JSON, SVG, an image or text); otherwise from
    /// the internal clipboard.
    pub fn paste_from_clipboard(&mut self) {
        use crate::io::clipboard::{self, Content};
        let external = self.clipboard_io.as_mut().and_then(|io| io.read_text());
        let center = self.camera.screen_to_world(self.viewport * 0.5);
        match external {
            Some(t) if self.clipboard_written.as_deref() != Some(t.as_str()) => {
                match clipboard::decode(&t) {
                    Content::Layers(p) => {
                        let same = p.page.as_deref() == Some(self.active_page_id());
                        let d = if same { PASTE_OFFSET } else { 0.0 };
                        let ids = self.insert_subtrees(
                            "Paste",
                            &p.nodes,
                            &p.roots,
                            &p.world,
                            v2(d, d),
                            false,
                        );
                        self.selection = ids;
                    }
                    Content::Foreign(nodes, what) => {
                        let (roots, world) = clipboard::roots_of(&nodes);
                        let off = clipboard::center_offset(&world, center);
                        let ids = self.insert_subtrees(
                            &format!("Paste {what}"),
                            &nodes,
                            &roots,
                            &world,
                            off,
                            false,
                        );
                        self.status = format!("Pasted {} layer(s) from {what}", ids.len());
                        self.selection = ids;
                    }
                    Content::Text(t) if !t.trim().is_empty() => {
                        let mut n =
                            crate::demo::text_node(t.trim_end(), 16.0, center.round(), Color::INK);
                        n.name = t
                            .lines()
                            .next()
                            .unwrap_or("Text")
                            .chars()
                            .take(24)
                            .collect();
                        let id = self.add_node(n, None);
                        self.selection = vec![id];
                    }
                    Content::Text(_) => {}
                }
            }
            _ => {
                let image = if external.is_none() {
                    self.clipboard_io.as_mut().and_then(|io| io.read_image())
                } else {
                    None
                };
                if let Some(img) = image {
                    let id = crate::assets::register_rgba(&img);
                    self.insert_image(&id, center);
                    return;
                }
                // Same page: offset so the copy is visible. Another page or project: keep
                // the position, as in Figma.
                let same = self.clipboard_page.as_deref() == Some(self.active_page_id());
                let d = if same { PASTE_OFFSET } else { 0.0 };
                self.paste(v2(d, d))
            }
        }
    }

    /// Put the selection on the clipboard as SVG markup.
    pub fn copy_as_svg(&mut self) {
        let roots = self.top_selection();
        if let Some(svg) =
            crate::io::svg::export(&self.doc, (!roots.is_empty()).then_some(&roots[..]), None)
            && let Some(io) = &mut self.clipboard_io
        {
            io.write_text(&svg);
            self.clipboard_written = Some(svg);
            self.status = "Copied as SVG".into();
        }
    }

    fn copy_internal(&mut self) {
        let roots = self.top_selection();
        if roots.is_empty() {
            return;
        }
        self.clipboard = roots
            .iter()
            .flat_map(|r| self.doc.subtree(*r))
            .filter_map(|id| self.doc.get(id).cloned())
            .collect();
        self.clipboard_world = roots
            .iter()
            .map(|r| self.doc.world_rect(*r).unwrap_or_default())
            .collect();
        self.clipboard_roots = roots;
        self.clipboard_page = Some(self.active_page_id().to_string());
        self.status = format!("Copied {} layer(s)", self.clipboard_roots.len());
    }

    fn paste(&mut self, offset: V2) {
        if self.clipboard_roots.is_empty() {
            return;
        }
        let nodes = self.clipboard.clone();
        let roots = self.clipboard_roots.clone();
        let same_page = self.clipboard_page.as_deref() == Some(self.active_page_id());
        let world = self.clipboard_world.clone();
        let new_roots = self.insert_subtrees("Paste", &nodes, &roots, &world, offset, same_page);
        self.selection = new_roots;
        self.status = format!("Pasted {} layer(s)", self.selection.len());
    }

    /// Insert copies of subtrees. `world[i]` is root `i`'s world rect; `same_page` keeps the
    /// original parents and references to layers outside the copied set.
    pub fn insert_subtrees(
        &mut self,
        label: &str,
        nodes: &[Node],
        roots: &[NodeId],
        world: &[Rect],
        offset: V2,
        same_page: bool,
    ) -> Vec<NodeId> {
        self.edit(label, |tx| {
            insert_subtrees_tx(tx, nodes, roots, world, offset, same_page)
        })
    }

    /// Insert a library item centered at world point `at`.
    pub fn insert_library_item(&mut self, item: &crate::library::LibItem, at: V2) -> Vec<NodeId> {
        let b = item.bounds().unwrap_or_default();
        let world: Vec<Rect> = item
            .roots
            .iter()
            .filter_map(|r| item.nodes.iter().find(|n| n.id == *r).map(|n| n.rect))
            .collect();
        let offset = (at - b.center()).round();
        let ids = self.insert_subtrees(
            &format!("Insert {}", item.name),
            &item.nodes,
            &item.roots,
            &world,
            offset,
            false,
        );
        self.selection = ids.clone();
        ids
    }

    fn reorder(&mut self, f: impl Fn(usize, usize) -> usize) {
        let ids = self.top_selection();
        if ids.is_empty() {
            return;
        }
        self.edit("Reorder", |tx| {
            for id in ids {
                let parent = tx.doc().get(id).and_then(|n| n.parent);
                let list = tx.doc().children(parent);
                let i = list.iter().position(|x| *x == id).unwrap();
                let len = list.len();
                let target = f(i, len - 1).min(len - 1);
                tx.move_to_index(id, target);
            }
        });
    }

    fn wrap_in_frame(&mut self) {
        let ids = self.top_selection();
        let Some(b) = self.selection_bounds() else {
            return;
        };
        let parent = self.doc.get(ids[0]).and_then(|n| n.parent);
        if ids
            .iter()
            .any(|id| self.doc.get(*id).and_then(|n| n.parent) != parent)
        {
            self.status = "Selection must share a parent".into();
            return;
        }
        let index = ids
            .iter()
            .filter_map(|id| self.doc.children(parent).iter().position(|x| x == id))
            .max()
            .map(|i| i + 1);
        let n_frames = self.doc.nodes.values().filter(|n| n.is_frame()).count();
        let frame = self.edit("Frame selection", |tx| {
            let fid = tx.alloc_id();
            let o = tx.doc().parent_origin(parent);
            let mut f = Node::new(
                fid,
                NodeKind::Frame { clip: true },
                b.translate(V2::ZERO - o),
            );
            f.name = format!("Frame {}", n_frames + 1);
            f.style.set_fill(None);
            tx.insert(f, parent, index);
            for id in &ids {
                tx.reparent(*id, Some(fid), None);
            }
            fid
        });
        self.selection = vec![frame];
    }

    // ------------------------------------------------------------------ programmatic API

    /// Place a registered image as a rectangle with an image fill, centered at `at` (world),
    /// at its natural size up to 1000 px on the long side.
    pub fn insert_image(&mut self, asset: &str, at: V2) -> Option<NodeId> {
        let a = crate::assets::get(asset)?;
        let (w, h) = (a.width as f64, a.height as f64);
        let s = (1000.0 / w.max(h)).min(1.0);
        let (w, h) = ((w * s).round(), (h * s).round());
        let parent = self.frame_at(at, &[]);
        let o = self.doc.parent_origin(parent);
        let mut n = Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(
                (at.x - w * 0.5).round() - o.x,
                (at.y - h * 0.5).round() - o.y,
                w,
                h,
            ),
        );
        n.name = "Image".into();
        n.style.fills = vec![crate::model::Paint::Image {
            asset: asset.into(),
            mode: crate::model::ImageMode::Fill,
            opacity: 1.0,
        }];
        let id = self.add_node(n, parent);
        self.selection = vec![id];
        Some(id)
    }

    /// Add a node at world position (used by tests, demos and the properties panel).
    pub fn add_node(&mut self, mut node: Node, parent: Option<NodeId>) -> NodeId {
        let label = format!("Add {}", node.kind.type_name().to_lowercase());
        self.begin(&label);
        let id = {
            let mut tx = self.tx();
            node.id = tx.alloc_id();
            tx.insert(node, parent, None)
        };
        self.fit_text(id);
        self.commit();
        id
    }
}

/// Insert copies of subtrees inside an open transaction (see `Editor::insert_subtrees`).
pub fn insert_subtrees_tx(
    tx: &mut Tx,
    nodes: &[Node],
    roots: &[NodeId],
    world: &[Rect],
    offset: V2,
    same_page: bool,
) -> Vec<NodeId> {
    let mut map = std::collections::HashMap::new();
    for n in nodes {
        map.insert(n.id, tx.alloc_id());
    }
    let remap = |c: &mut Node, map: &std::collections::HashMap<NodeId, NodeId>| {
        if let NodeKind::Connector { start, end, .. } = &mut c.kind {
            for b in [start, end] {
                if let Some(bb) = b {
                    match map.get(&bb.node) {
                        Some(n) => bb.node = *n,
                        None if same_page => {}
                        None => *b = None,
                    }
                }
            }
        }
        if let Some(l) = &mut c.instance {
            match map.get(&l.main) {
                Some(n) => l.main = *n,
                None if same_page => {}
                None => c.instance = None,
            }
        }
    };
    let mut new_roots = Vec::new();
    for (i, r) in roots.iter().enumerate() {
        let Some(n) = nodes.iter().find(|n| n.id == *r) else {
            continue;
        };
        // Same page: next to the original if its parent still exists. Elsewhere: root.
        let parent = n
            .parent
            .filter(|p| same_page && tx.doc().nodes.contains_key(p));
        let w = world.get(i).copied().unwrap_or(n.rect);
        let mut copy = n.clone();
        copy.id = map[&n.id];
        copy.children = n.children.iter().map(|c| map[c]).collect();
        copy.rect = w.translate(offset - tx.doc().parent_origin(parent));
        remap(&mut copy, &map);
        let id = tx.insert(copy, parent, None);
        for d in nodes
            .iter()
            .filter(|d| d.id != n.id && is_desc(nodes, d.id, n.id))
        {
            let mut c = d.clone();
            c.id = map[&d.id];
            c.parent = d.parent.map(|p| map[&p]);
            c.children = d.children.iter().map(|x| map[x]).collect();
            remap(&mut c, &map);
            tx.put(c);
        }
        new_roots.push(id);
    }
    for id in map.values() {
        crate::editor::whiteboard::update_connector(tx, *id);
    }
    new_roots
}

/// A fresh process-wide document stamp.
pub fn next_stamp() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn is_desc(nodes: &[Node], id: NodeId, ancestor: NodeId) -> bool {
    let mut cur = nodes.iter().find(|n| n.id == id).and_then(|n| n.parent);
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = nodes.iter().find(|n| n.id == p).and_then(|n| n.parent);
    }
    false
}

fn dedup(mut v: Vec<NodeId>) -> Vec<NodeId> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(*x));
    v
}

#[cfg(test)]
mod tests;
