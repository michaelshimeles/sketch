//! Element feedback with explicit target context for agent batches.
use super::*;
use crate::model::Comment;

fn comment_json(ed: &Editor, page: usize, c: &Comment) -> Value {
    let doc = ed.page_doc(page);
    let node = doc.get(c.target);
    let at = c.position(doc);
    let bounds = doc
        .world_rect(c.target)
        .map(|r| json!({"x":r.x,"y":r.y,"width":r.w,"height":r.h}));
    json!({
        "id":c.id,"body":c.body,"status":if c.resolved {"resolved"} else {"open"},
        "pageId":ed.pages[page].id,"pageName":ed.pages[page].name,
        "nodeId":node_key(ed,page,c.target),"targetMissing":node.is_none(),
        "target":{"name":node.map(|n|n.name.as_str()).unwrap_or(&c.target_name),"text":node.and_then(|n|n.kind.text()),
            "ancestors":doc.ancestors(c.target).iter().filter_map(|id|doc.get(*id)).map(|n|n.name.clone()).collect::<Vec<_>>(),"bounds":bounds},
        "position":{"x":at.x,"y":at.y},"createdAt":c.created_at,"updatedAt":c.updated_at
    })
}
fn locate(ed: &Editor, id: &str) -> Result<(usize, usize), String> {
    for page in 0..ed.pages.len() {
        if let Some(i) = ed.page_doc(page).comments.iter().position(|c| c.id == id) {
            return Ok((page, i));
        }
    }
    Err("comment does not exist".into())
}
fn integer(a: &Args, name: &str, default: usize) -> Result<usize, String> {
    match a.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| format!("{name} must be a nonnegative integer")),
    }
}
pub(super) fn call(ed: &mut Editor, name: &str, a: &Args) -> ToolResult {
    if name == "list_comments" {
        let page = a.str("pageId").map(|p| find_page(ed, p)).transpose()?;
        let status = a.str("status").unwrap_or("open");
        if !["open", "resolved", "all"].contains(&status) {
            return Err("status must be open, resolved, or all".into());
        }
        let limit = integer(a, "limit", 50)?;
        let offset = integer(a, "offset", 0)?;
        if !(1..=500).contains(&limit) {
            return Err("limit must be between 1 and 500".into());
        }
        let mut values = Vec::new();
        for p in 0..ed.pages.len() {
            if page.is_some_and(|q| q != p) {
                continue;
            }
            for c in &ed.page_doc(p).comments {
                if status == "all" || (status == "resolved") == c.resolved {
                    values.push(comment_json(ed, p, c));
                }
            }
        }
        let total = values.len();
        return text(
            json!({"total":total,"comments":values.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"offset":offset,"limit":limit}),
        );
    }
    if name == "add_comment" {
        let (page, target) = resolve(ed, a.req_str("nodeId")?, page_arg(ed, a)?)?;
        let r = ed.page_doc(page).world_rect(target).unwrap();
        let number = |key: &str| {
            a.get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("{key} must be a finite number"))
        };
        let at = match (a.get("x"), a.get("y")) {
            (None, None) => r.center(),
            (Some(_), Some(_)) => v2(number("x")?, number("y")?),
            _ => return Err("provide both x and y, or neither".into()),
        };
        let body = a.req_str("body")?;
        let id = ed.with_page(page, |ed| ed.add_comment(target, at, body))?;
        let (_, i) = locate(ed, &id)?;
        return text(comment_json(ed, page, &ed.page_doc(page).comments[i]));
    }
    let id = a.req_str("commentId")?;
    let (page, i) = locate(ed, id)?;
    if let Some(expected) = a.get("expectedUpdatedAt") {
        let expected = expected
            .as_u64()
            .ok_or("expectedUpdatedAt must be an integer")?;
        if ed.page_doc(page).comments[i].updated_at != expected {
            return Err("comment changed since it was read; leave it open and reread it".into());
        }
    }
    match name {
        "get_comment" => {}
        "reanchor_comment" => {
            if a.get("expectedUpdatedAt").is_none() {
                return Err("expectedUpdatedAt is required for anchor changes".into());
            }
            let (target_page, target) = resolve(ed, a.req_str("nodeId")?, page)?;
            if target_page != page {
                return Err("refresh anchors must stay on the original page".into());
            }
            ed.with_page(page, |ed| ed.reanchor_comment(id, target))?;
        }
        "update_comment" => {
            let body = a.req_str("body")?;
            ed.with_page(page, |ed| ed.update_comment(id, body))?;
        }
        "set_comment_status" => {
            let resolved = match a.req_str("status")? {
                "open" => false,
                "resolved" => true,
                _ => return Err("status must be open or resolved".into()),
            };
            ed.with_page(page, |ed| ed.resolve_comment(id, resolved))?;
        }
        "delete_comment" => {
            ed.with_page(page, |ed| ed.delete_comment(id))?;
            return text(json!({"deleted":true}));
        }
        _ => return Err("unknown comment operation".into()),
    }
    text(comment_json(ed, page, &ed.page_doc(page).comments[i]))
}
