use serde_json::{Value, json};
use sketch_rs::mcp::{Out, exec};
use sketch_rs::project::Store;

fn call(host: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    let result = exec::call(host, name, &args).unwrap_or_else(|e| panic!("{name}: {e}"));
    match &result[0] {
        Out::Text(text) => serde_json::from_str(text).unwrap(),
        _ => panic!("expected text"),
    }
}

#[test]
fn independent_workspaces_keep_visible_project_and_undo_separate() {
    let root = std::env::temp_dir().join(format!("sketch-workspaces-{:016x}", fastrand::u64(..)));
    let store = Store::open(&root).unwrap();
    let mut host = exec::SessionHost::new(store);
    host.session.create(&mut host.ed, "Main design").unwrap();
    let source = host.session.project.as_ref().unwrap().manifest.id.clone();
    let page = host.ed.active_page_id().to_owned();
    let a = call(
        &mut host,
        "create_workspace",
        json!({"fileId":source,"name":"Navigation"}),
    );
    let b = call(
        &mut host,
        "create_workspace",
        json!({"fileId":source,"name":"Dashboard"}),
    );
    let wa = a["workspaceId"].clone();
    let wb = b["workspaceId"].clone();
    for (workspace, name) in [(&wa, "Sidebar"), (&wb, "Metrics"), (&wa, "Menu")] {
        call(
            &mut host,
            "create_shape",
            json!({"workspaceId":workspace,"type":"rectangle","name":name}),
        );
    }
    assert_eq!(host.ed.active_page_id(), page);
    assert!(host.ed.doc.nodes.is_empty());
    assert_eq!(host.session.project.as_ref().unwrap().manifest.id, source);
    call(&mut host, "undo", json!({"workspaceId":wa}));
    let ia = call(&mut host, "get_basic_info", json!({"workspaceId":wa}));
    let ib = call(&mut host, "get_basic_info", json!({"workspaceId":wb}));
    assert_ne!(ia["fileId"], ib["fileId"]);
    call(
        &mut host,
        "finish_working_on_nodes",
        json!({"workspaceId":wa}),
    );
    call(
        &mut host,
        "finish_working_on_nodes",
        json!({"workspaceId":wb}),
    );
    let pa = sketch_rs::project::Project::open(std::path::Path::new(a["path"].as_str().unwrap()))
        .unwrap();
    let pb = sketch_rs::project::Project::open(std::path::Path::new(b["path"].as_str().unwrap()))
        .unwrap();
    assert_eq!(pa.load_pages().unwrap()[0].doc.nodes.len(), 1);
    assert_eq!(pb.load_pages().unwrap()[0].doc.nodes.len(), 1);
    drop(host);
    std::fs::remove_dir_all(root).unwrap();
}

fn fixture() -> (exec::SessionHost, std::path::PathBuf, String) {
    let root = std::env::temp_dir().join(format!("sketch-workspaces-{:016x}", fastrand::u64(..)));
    let store = Store::open(&root).unwrap();
    let mut host = exec::SessionHost::new(store);
    host.session.create(&mut host.ed, "Design").unwrap();
    let id = host.session.project.as_ref().unwrap().manifest.id.clone();
    (host, root, id)
}

#[test]
fn inspecting_and_leaving_workspace_preserves_history_and_survives_restart() {
    let (mut host, root, source) = fixture();
    let w = call(
        &mut host,
        "create_workspace",
        json!({"fileId":source,"name":"Option A"}),
    );
    let wid = w["workspaceId"].clone();
    call(
        &mut host,
        "create_shape",
        json!({"workspaceId":wid,"type":"rectangle","name":"First"}),
    );
    call(&mut host, "open_file", json!({"fileId":wid}));
    assert_eq!(host.ed.doc.nodes.len(), 1);
    call(
        &mut host,
        "create_shape",
        json!({"workspaceId":wid,"type":"rectangle","name":"Second"}),
    );
    assert_eq!(host.ed.doc.nodes.len(), 2);
    call(&mut host, "open_file", json!({"fileId":source}));
    call(&mut host, "undo", json!({"workspaceId":wid}));
    call(&mut host, "open_file", json!({"fileId":wid}));
    assert_eq!(host.ed.doc.nodes.len(), 1, "history survives inspection");
    let store = host.session.store.clone();
    drop(host);
    let mut restarted = exec::SessionHost::new(store);
    assert_eq!(
        call(&mut restarted, "list_workspaces", json!({"fileId":source}))["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let info = call(&mut restarted, "get_basic_info", json!({"workspaceId":wid}));
    assert_eq!(info["fileId"], wid);
    call(&mut restarted, "open_file", json!({"fileId":wid}));
    assert_eq!(restarted.ed.doc.nodes.len(), 1);
    drop(restarted);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fork_includes_unsaved_edits_and_import_is_additive_with_fresh_comment_ids() {
    let (mut host, root, source) = fixture();
    let shape = call(
        &mut host,
        "create_shape",
        json!({"type":"rectangle","name":"Original"}),
    )["id"]
        .clone();
    call(
        &mut host,
        "add_comment",
        json!({"nodeId":shape,"body":"Keep this"}),
    );
    assert!(host.ed.project_dirty());
    let w = call(
        &mut host,
        "create_workspace",
        json!({"fileId":source,"name":"Alternative"}),
    );
    let original_page = host.ed.active_page_id().to_owned();
    let original_comment = host.ed.doc.comments[0].id.clone();
    let wid = w["workspaceId"].clone();
    call(
        &mut host,
        "create_shape",
        json!({"workspaceId":wid,"type":"ellipse","name":"Added"}),
    );
    let imported = call(
        &mut host,
        "import_workspace_pages",
        json!({"fileId":source,"sourceWorkspaceId":wid,"pageIds":[original_page]}),
    );
    assert_eq!(host.ed.active_page_id(), original_page);
    assert_eq!(host.ed.doc.nodes.len(), 1);
    assert_eq!(host.ed.pages.len(), 2);
    let page = host.ed.page_data(1);
    assert_ne!(page.id, original_page);
    assert_ne!(page.doc.comments[0].id, original_comment);
    assert_eq!(page.doc.nodes.len(), 2);
    assert_eq!(page.doc.comments[0].body, "Keep this");
    assert_eq!(imported["imported"][0]["pageId"], page.id);
    // Import keeps the destination page's existing undo history.
    call(&mut host, "undo", json!({}));
    assert!(host.ed.doc.comments.is_empty());
    assert_eq!(host.ed.doc.nodes.len(), 1);
    drop(host);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bad_workspace_and_file_mismatch_never_fall_back_to_visible_document() {
    let (mut host, root, source) = fixture();
    let w = call(
        &mut host,
        "create_workspace",
        json!({"fileId":source,"name":"Isolated"}),
    );
    for id in [Value::Null, json!(""), json!("missing"), json!(3)] {
        assert!(
            exec::call(
                &mut host,
                "create_shape",
                &json!({"workspaceId":id,"type":"rectangle"})
            )
            .is_err()
        );
    }
    assert!(
        exec::call(
            &mut host,
            "create_shape",
            &json!({"workspaceId":w["workspaceId"],"fileId":source,"type":"rectangle"})
        )
        .is_err()
    );
    assert!(
        exec::call(
            &mut host,
            "open_file",
            &json!({"workspaceId":w["workspaceId"],"fileId":source})
        )
        .is_err()
    );
    assert!(
        exec::call(
            &mut host,
            "import_workspace_pages",
            &json!({"fileId":source,"sourceWorkspaceId":w["workspaceId"],"pageIds":["missing"]})
        )
        .is_err()
    );
    assert_eq!(host.ed.pages.len(), 1);
    assert!(host.ed.doc.nodes.is_empty());
    drop(host);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn imported_image_assets_are_saved_with_destination() {
    let (mut host, root, source) = fixture();
    let w = call(
        &mut host,
        "create_workspace",
        json!({"fileId":source,"name":"Images"}),
    );
    let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([40, 100, 160, 255]));
    let asset = sketch_rs::assets::register_rgba(&img);
    let asset_path = root.join("input.png");
    img.save(&asset_path).unwrap();
    let html = format!(
        "<img src=\"{}\" style=\"width:30px;height:20px\"/>",
        asset_path.display()
    );
    call(
        &mut host,
        "write_html",
        json!({"workspaceId":w["workspaceId"],"targetNodeId":"root","mode":"insert-children","html":html}),
    );
    let page = host.ed.active_page_id().to_owned();
    call(
        &mut host,
        "import_workspace_pages",
        json!({"fileId":source,"sourceWorkspaceId":w["workspaceId"],"pageIds":[page]}),
    );
    let destination = host.session.project.as_ref().unwrap().dir.clone();
    assert!(
        destination
            .join("assets")
            .join(format!("{asset}.png"))
            .is_file()
    );
    assert_eq!(
        sketch_rs::assets::used_by(&host.ed.page_data(1).doc),
        vec![asset]
    );
    drop(host);
    std::fs::remove_dir_all(root).unwrap();
}
