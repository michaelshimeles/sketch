use serde_json::{Value, json};
use sketch_rs::mcp::{Out, exec};
use sketch_rs::project::Store;

fn host() -> exec::SessionHost {
    let store = Store::open(
        std::env::temp_dir().join(format!("sketch-comments-{:016x}", fastrand::u64(..))),
    )
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Comments test").unwrap();
    h
}
fn call(h: &mut exec::SessionHost, tool: &str, args: Value) -> Value {
    let out = exec::call(h, tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"));
    match &out[0] {
        Out::Text(t) => serde_json::from_str(t).unwrap(),
        _ => panic!("expected text"),
    }
}
fn target(h: &mut exec::SessionHost) -> Value {
    call(
        h,
        "create_artboard",
        json!({"name":"Card", "styles":{"width":200,"height":100,"left":40,"top":60}}),
    )["id"]
        .clone()
}
#[test]
fn comments_follow_targets_and_survive_delete_undo_and_save() {
    let mut h = host();
    let node = target(&mut h);
    let c = call(
        &mut h,
        "add_comment",
        json!({"nodeId":node,"body":"Make this clearer", "x":90,"y":85}),
    );
    let id = c["id"].clone();
    assert_eq!(c["target"]["name"], "Card");
    assert_eq!(c["position"], json!({"x":90.0,"y":85.0}));
    call(
        &mut h,
        "update_styles",
        json!({"updates":[{"nodeIds":[node],"styles":{"left":140,"top":160,"width":400,"height":200}}]}),
    );
    let c = call(&mut h, "get_comment", json!({"commentId":id}));
    assert_eq!(c["position"], json!({"x":240.0,"y":210.0}));
    call(&mut h, "delete_nodes", json!({"nodeIds":[node]}));
    assert_eq!(
        call(&mut h, "get_comment", json!({"commentId":id}))["targetMissing"],
        true
    );
    call(&mut h, "undo", json!({}));
    assert_eq!(
        call(&mut h, "get_comment", json!({"commentId":id}))["targetMissing"],
        false
    );
    call(
        &mut h,
        "set_comment_status",
        json!({"commentId":id,"status":"resolved"}),
    );
    call(
        &mut h,
        "update_comment",
        json!({"commentId":id,"body":"Updated feedback"}),
    );
    let path = h.session.project.as_ref().unwrap().dir.clone();
    call(&mut h, "finish_working_on_nodes", json!({}));
    let mut other = exec::SessionHost::new(h.session.store.clone());
    other.session.open(&mut other.ed, &path).unwrap();
    let c = call(&mut other, "get_comment", json!({"commentId":id}));
    assert_eq!(c["body"], "Updated feedback");
    assert_eq!(c["status"], "resolved");
    assert_eq!(call(&mut other, "list_comments", json!({}))["total"], 0);
    assert!(
        !sketch_rs::io::svg::export(&other.ed.doc, None, None)
            .unwrap()
            .contains("Updated feedback")
    );
}

#[test]
fn comments_paginate_across_pages_and_page_copies_are_independent() {
    let mut h = host();
    let n = target(&mut h);
    let first = call(&mut h, "add_comment", json!({"nodeId":n,"body":"First"}));
    let page = h.ed.active_page_id().to_string();
    let dup = h.ed.duplicate_page(0);
    assert_eq!(dup, 1);
    let list = call(&mut h, "list_comments", json!({"limit":1,"offset":1}));
    assert_eq!(list["total"], 2);
    let copied = list["comments"][0]["id"].clone();
    assert_ne!(copied, first["id"]);
    call(
        &mut h,
        "set_comment_status",
        json!({"commentId":copied,"status":"resolved"}),
    );
    assert_eq!(
        call(&mut h, "list_comments", json!({"pageId":page}))["total"],
        1
    );
    assert_eq!(
        call(&mut h, "list_comments", json!({"status":"all"}))["total"],
        2
    );
    call(&mut h, "undo", json!({}));
    assert_eq!(call(&mut h, "list_comments", json!({}))["total"], 2);
}
#[test]
fn invalid_comment_requests_do_not_mutate_the_project() {
    let mut h = host();
    let n = target(&mut h);
    for args in [
        json!({"nodeId":n,"body":"   "}),
        json!({"nodeId":"not-a-node","body":"Text"}),
        json!({"nodeId":n,"body":"Text","x":20}),
        json!({"nodeId":n,"body":"Text","x":-999,"y":20}),
    ] {
        assert!(exec::call(&mut h, "add_comment", &args).is_err());
    }
    assert_eq!(call(&mut h, "list_comments", json!({}))["total"], 0);
    let c = call(&mut h, "add_comment", json!({"nodeId":n,"body":"Valid"}));
    let before = h.ed.doc.clone();
    for (tool, args) in [
        (
            "set_comment_status",
            json!({"commentId":c["id"],"status":"bad"}),
        ),
        ("update_comment", json!({"commentId":c["id"],"body":""})),
        ("delete_comment", json!({"commentId":"missing"})),
        ("list_comments", json!({"limit":-1})),
    ] {
        assert!(exec::call(&mut h, tool, &args).is_err());
        assert_eq!(h.ed.doc, before);
    }
}

#[test]
fn comment_shortcut_does_not_move_design_elements() {
    use sketch_rs::editor::{InputEvent, Key};
    let mut h = host();
    target(&mut h);
    let before = h.ed.doc.clone();
    h.ed.handle(InputEvent::KeyDown {
        key: Key::Char('c'),
        repeat: false,
    });
    assert_eq!(h.ed.tool.label(), "Comment");
    assert_eq!(h.ed.doc, before);
}

#[test]
fn comment_mode_targets_locked_elements_and_preserves_a_draft() {
    use sketch_rs::editor::{Button, InputEvent, Key};
    use sketch_rs::geom::v2;
    let mut h = host();
    let n = target(&mut h);
    let id = sketch_rs::model::NodeId(
        n.as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    );
    h.ed.edit("Lock", |tx| tx.node_mut(id).unwrap().locked = true);
    let before = h.ed.doc.clone();
    h.ed.handle(InputEvent::KeyDown {
        key: Key::Char('c'),
        repeat: false,
    });
    let pos = h.ed.camera.world_to_screen(v2(90.0, 85.0));
    h.ed.handle(InputEvent::PointerMove {
        pos,
        pressure: None,
        time_ms: 0.0,
    });
    assert_eq!(h.ed.hover, Some(id));
    h.ed.handle(InputEvent::PointerDown {
        pos,
        button: Button::Primary,
        pressure: None,
        time_ms: 1.0,
    });
    assert!(h.ed.comment_draft.is_some());
    h.ed.comment_draft.as_mut().unwrap().text = "Keep my draft".into();
    h.ed.handle(InputEvent::PointerDown {
        pos: h.ed.camera.world_to_screen(v2(150.0, 100.0)),
        button: Button::Primary,
        pressure: None,
        time_ms: 500.0,
    });
    assert_eq!(h.ed.comment_draft.as_ref().unwrap().text, "Keep my draft");
    assert_eq!(h.ed.doc, before);
    h.ed.handle(InputEvent::KeyDown {
        key: Key::Escape,
        repeat: false,
    });
    assert!(h.ed.comment_draft.is_none());
    h.ed.edit("Hide", |tx| tx.node_mut(id).unwrap().visible = false);
    h.ed.handle(InputEvent::PointerMove {
        pos,
        pressure: None,
        time_ms: 600.0,
    });
    assert_eq!(h.ed.hover, None);
}
#[test]
fn comments_have_versioned_storage_and_legacy_export_keeps_metadata() {
    let mut h = host();
    let n = target(&mut h);
    call(
        &mut h,
        "add_comment",
        json!({"nodeId":n,"body":"Persist me"}),
    );
    let serialized = sketch_rs::io::file::to_string(&h.ed.doc, None);
    let (doc, _) = sketch_rs::io::file::from_str(&serialized).unwrap();
    assert_eq!(doc.comments.len(), 1);
    assert_eq!(doc.comments[0].body, "Persist me");
    let page = sketch_rs::project::format::page_to_json(&h.ed.page_data(0));
    let v: Value = serde_json::from_str(&page).unwrap();
    assert!(v["version"].as_u64().unwrap() > 3);
    assert_eq!(v["comments"][0]["body"], "Persist me");
}

#[test]
fn near_edge_comments_cannot_create_unreadable_saved_pages() {
    let mut h = host();
    let n = target(&mut h);
    assert!(
        exec::call(
            &mut h,
            "add_comment",
            &json!({"nodeId":n,"body":"Edge","x":39.9995,"y":85})
        )
        .is_err()
    );
    assert_eq!(call(&mut h, "list_comments", json!({}))["total"], 0);
}
#[test]
fn opening_a_comment_ends_canvas_text_editing() {
    let mut h = host();
    let n = call(
        &mut h,
        "write_html",
        json!({"targetNodeId":"root","mode":"insert-children","html":"<p>Hello</p>"}),
    )["createdNodeIds"][0]
        .clone();
    let c = call(
        &mut h,
        "add_comment",
        json!({"nodeId":n,"body":"Text feedback"}),
    );
    let id = sketch_rs::model::NodeId(
        n.as_str()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    );
    h.ed.start_text_edit(id, None);
    assert!(h.ed.text_edit.is_some());
    h.ed.focus_comment(c["id"].as_str().unwrap());
    assert!(h.ed.text_edit.is_none());
}

#[test]
fn fractional_bounds_comments_reopen_after_save() {
    let mut h = host();
    let node = call(
        &mut h,
        "create_artboard",
        json!({"name":"Fractional card","styles":{"left":200,"top":200,"width":100.1,"height":100.1}}),
    )["id"].clone();
    call(
        &mut h,
        "add_comment",
        json!({"nodeId":node,"body":"At the corner","x":300.1,"y":300.1}),
    );
    let serialized = sketch_rs::project::format::page_to_json(&h.ed.page_data(0));
    sketch_rs::project::format::page_from_json(&serialized).unwrap();
}
