//! Independent document sessions for agent workspaces. The window may adopt a session
//! for inspection; switching away parks that same editor, including its undo history.

use super::exec::{Host, SessionHost};
use super::{Out, ToolResult};
use crate::editor::Editor;
use crate::project::{PageData, Project, Store, new_id};
use crate::session::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const METADATA: &str = "workspace.json";

#[derive(Serialize, Deserialize)]
pub struct Metadata {
    pub workspace_id: String,
    pub source_file_id: String,
    pub name: String,
}

pub fn metadata(dir: &Path) -> Option<Metadata> {
    let meta: Metadata = serde_json::from_slice(&std::fs::read(dir.join(METADATA)).ok()?).ok()?;
    let project = Project::open(dir).ok()?;
    (meta.workspace_id == project.manifest.id).then_some(meta)
}

#[derive(Default)]
pub struct Workspaces {
    sessions: HashMap<PathBuf, SessionHost>,
}

impl Workspaces {
    /// Load the destination before replacing the visible document. Only workspace
    /// editors are retained; ordinary projects keep their existing open/close behavior.
    pub fn open(
        &mut self,
        ed: &mut Editor,
        session: &mut Session,
        dir: &Path,
    ) -> Result<(), String> {
        let dir = dir.canonicalize().map_err(|e| e.to_string())?;
        if session
            .project
            .as_ref()
            .is_some_and(|p| same_dir(&p.dir, &dir))
        {
            return Ok(());
        }
        session.save_all(ed).map_err(|e| e.to_string())?;
        let mut incoming = self.take(&session.store, &dir)?;
        // The OS clipboard provider belongs to the window, not a parked editor.
        incoming.ed.clipboard_io = ed.clipboard_io.take();
        std::mem::swap(ed, &mut incoming.ed);
        std::mem::swap(session, &mut incoming.session);
        self.park(incoming);
        Ok(())
    }

    pub fn leave(&mut self, ed: &mut Editor, session: &mut Session) -> Result<(), String> {
        session.save_all(ed).map_err(|e| e.to_string())?;
        let mut parked = SessionHost::new(session.store.clone());
        parked.ed.clipboard_io = ed.clipboard_io.take();
        std::mem::swap(ed, &mut parked.ed);
        std::mem::swap(session, &mut parked.session);
        self.park(parked);
        Ok(())
    }

    fn take(&mut self, store: &Store, dir: &Path) -> Result<SessionHost, String> {
        let key = dir.canonicalize().map_err(|e| e.to_string())?;
        if let Some(host) = self.sessions.remove(&key) {
            // Renamed or deleted projects must not be resurrected by a cached session.
            let disk = Project::open(&key).map_err(|e| e.to_string())?;
            if host
                .session
                .project
                .as_ref()
                .is_some_and(|p| p.manifest == disk.manifest)
            {
                return Ok(host);
            }
        }
        let mut host = SessionHost::new(store.clone());
        host.session
            .open(&mut host.ed, &key)
            .map_err(|e| e.to_string())?;
        Ok(host)
    }

    fn park(&mut self, host: SessionHost) {
        if let Some(p) = &host.session.project
            && metadata(&p.dir).is_some()
        {
            self.sessions
                .insert(p.dir.canonicalize().unwrap_or_else(|_| p.dir.clone()), host);
        }
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn text(value: Value) -> ToolResult {
    Ok(vec![Out::Text(
        serde_json::to_string_pretty(&value).unwrap(),
    )])
}

fn required<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing nonempty string `{name}`"))
}

fn resolve(store: &Store, id: &str) -> Result<PathBuf, String> {
    // Workspace IDs, unlike fileId, never fall back to names or the visible document.
    store
        .list()
        .into_iter()
        .find(|p| p.manifest.id == id && metadata(&p.dir).is_some_and(|m| m.workspace_id == id))
        .map(|p| p.dir)
        .ok_or_else(|| format!("workspace `{id}` not found (see list_workspaces)"))
}

/// Route a call without changing the window's selected project or page.
pub fn route(host: &mut dyn Host, name: &str, args: &Value) -> ToolResult {
    let id = required(args, "workspaceId")?;
    if matches!(
        name,
        "open_file"
            | "create_file"
            | "list_resources"
            | "create_workspace"
            | "list_workspaces"
            | "import_workspace_pages"
    ) {
        return Err(format!(
            "`{name}` does not accept workspaceId; use fileId for project management"
        ));
    }
    let store = host.store().clone();
    let dir = resolve(&store, id)?;
    if host.canvas_open() && host.project().is_some_and(|p| same_dir(&p.dir, &dir)) {
        let result = super::exec::call_document(host, name, args);
        // Workspace calls are durable even when the window is inspecting this workspace.
        host.save()?;
        return result;
    }
    let mut background = host.workspaces().take(&store, &dir)?;
    let result = super::exec::call_document(&mut background, name, args);
    let saved = background.save();
    host.workspaces().park(background);
    saved?;
    result
}

pub fn create(host: &mut dyn Host, args: &Value) -> ToolResult {
    let name = required(args, "name")?.trim();
    let key = required(args, "fileId")?;
    let dir = super::exec::find_project(host.store(), key)?;
    let pages = snapshot(host, &dir)?;
    let source = Project::open(&dir).map_err(|e| e.to_string())?;
    let project_name = format!("{} - {}", source.manifest.name, name);
    let project = host
        .store()
        .create_with_pages(&project_name, pages)
        .map_err(|e| e.to_string())?;
    let meta = Metadata {
        workspace_id: project.manifest.id.clone(),
        source_file_id: source.manifest.id,
        name: name.into(),
    };
    crate::io::atomic::write(
        &project.dir.join(METADATA),
        &serde_json::to_vec_pretty(&meta).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    text(
        json!({"workspaceId":meta.workspace_id,"fileId":project.manifest.id,
        "sourceFileId":meta.source_file_id,"name":meta.name,"path":project.dir,
        "next":"Pass workspaceId on every editing call. open_file is only for showing a workspace in the window."}),
    )
}

fn snapshot(host: &mut dyn Host, dir: &Path) -> Result<Vec<PageData>, String> {
    if host.project().is_some_and(|p| same_dir(&p.dir, dir)) {
        host.save()?;
    }
    // Background calls save before returning, so disk holds their latest completed edit.
    let project = Project::open(dir).map_err(|e| e.to_string())?;
    crate::assets::load_dir(&dir.join(crate::project::ASSETS_DIR));
    project.load_pages().map_err(|e| e.to_string())
}

pub fn list(host: &mut dyn Host, args: &Value) -> ToolResult {
    let source = args
        .get("fileId")
        .map(|_| required(args, "fileId"))
        .transpose()?
        .map(|key| {
            super::exec::find_project(host.store(), key).and_then(|dir| {
                Project::open(&dir)
                    .map(|p| p.manifest.id)
                    .map_err(|e| e.to_string())
            })
        })
        .transpose()?;
    let items: Vec<Value> = host.store().list().into_iter().filter_map(|p| {
        let m = metadata(&p.dir)?;
        if source.as_ref().is_some_and(|id| *id != m.source_file_id) { return None; }
        Some(json!({"workspaceId":p.manifest.id,"name":m.name,"sourceFileId":m.source_file_id,
            "path":p.dir,"visible":host.canvas_open() && host.project().is_some_and(|open| same_dir(&open.dir,&p.dir))}))
    }).collect();
    text(json!({"items":items}))
}

/// Imports are additive. Existing pages, their histories and comment anchors stay intact.
pub fn import_pages(host: &mut dyn Host, args: &Value) -> ToolResult {
    let target = super::exec::find_project(host.store(), required(args, "fileId")?)?;
    if !host.canvas_open() || !host.project().is_some_and(|p| same_dir(&p.dir, &target)) {
        return Err("Open the destination project before importing workspace pages".into());
    }
    let source = resolve(host.store(), required(args, "sourceWorkspaceId")?)?;
    if same_dir(&source, &target) {
        return Err("Choose a destination other than the source workspace".into());
    }
    let ids = args
        .get("pageIds")
        .and_then(Value::as_array)
        .filter(|ids| !ids.is_empty())
        .ok_or("pageIds must contain at least one page id")?;
    let pages = snapshot(host, &source)?;
    let mut selected = Vec::new();
    for id in ids {
        let id = id.as_str().ok_or("pageIds must contain strings")?;
        let page = pages
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("page `{id}` not found"))?;
        if selected.iter().any(|p: &PageData| p.id == id) {
            return Err("pageIds contains a duplicate".into());
        }
        selected.push(page.clone());
    }
    let ed = host.editor();
    ed.commit_pending();
    let mut imported = Vec::new();
    for mut page in selected {
        let i = ed.add_page_in_background(Some(&format!("{} imported", page.name)));
        let slot = &mut ed.pages[i];
        for comment in &mut page.doc.comments {
            comment.id = new_id();
        }
        page.doc.name = slot.name.clone();
        slot.stash = Some(Box::new(crate::editor::pages::PageStash {
            doc: page.doc,
            history: Default::default(),
            camera: page.camera.unwrap_or_default(),
            selection: Vec::new(),
            needs_fit: page.camera.is_none(),
        }));
        imported.push(json!({"sourcePageId":page.id,"pageId":slot.id,"name":slot.name}));
    }
    host.save()?;
    text(json!({"imported":imported}))
}
