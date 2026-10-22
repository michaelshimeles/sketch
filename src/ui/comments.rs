//! Element highlights, pinned feedback, and the batch review panel.
use crate::editor::{Editor, Tool, comments::Draft};
use crate::geom::v2;
use crate::theme::{self, Icon};
use egui::{Align2, Color32, FontId, RichText, Stroke};

#[derive(Default)]
pub struct CommentsUi {
    pub open: bool,
    pub show_resolved: bool,
}

pub fn panel(ui: &mut egui::Ui, ed: &mut Editor, st: &mut CommentsUi) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Comments").strong().size(16.0));
        if ui.small_button("Close").clicked() && ed.comment_draft.is_none() {
            st.open = false;
            ed.tool = Tool::Select;
            ed.focused_comment = None;
        }
    });
    ui.add_space(8.0);
    ui.label(RichText::new("Point to an element. Leave your feedback.").weak());
    ui.add_space(8.0);
    ui.checkbox(&mut st.show_resolved, "Include resolved");
    ui.separator();
    let rows: Vec<_> = (0..ed.pages.len())
        .flat_map(|p| {
            ed.page_doc(p)
                .comments
                .iter()
                .filter(|c| st.show_resolved || !c.resolved)
                .map(|c| {
                    (
                        p,
                        ed.pages[p].name.clone(),
                        c.clone(),
                        ed.page_doc(p).get(c.target).map(|n| n.name.clone()),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if rows.is_empty() {
        ui.add_space(16.0);
        ui.label("No open comments");
        ui.label(RichText::new("Press C, then click an element to add one. Add a whole batch before asking your agent to apply it.").weak());
    }
    egui::ScrollArea::vertical()
        .id_salt("comments-list")
        .show(ui, |ui| {
            for (_, page, c, target_name) in rows {
                ui.push_id(&c.id, |ui| {
                    let active = ed.focused_comment.as_ref() == Some(&c.id);
                    egui::Frame::NONE
                        .fill(if active {
                            theme::pal(ui).pressed
                        } else {
                            Color32::TRANSPARENT
                        })
                        .inner_margin(10.0)
                        .corner_radius(8.0)
                        .show(ui, |ui| {
                            let label = format!(
                                "{} · {}",
                                page,
                                target_name.as_deref().unwrap_or("Element removed")
                            );
                            if ui
                                .selectable_label(active, RichText::new(label).small())
                                .clicked()
                            {
                                ed.focus_comment(&c.id);
                            }
                            ui.label(&c.body);
                            if c.resolved {
                                ui.label(RichText::new("Resolved").small().weak());
                            }
                            ui.add_enabled_ui(ed.comment_draft.is_none(), |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .small_button(if c.resolved { "Reopen" } else { "Resolve" })
                                        .clicked()
                                        && let Some(p) = (0..ed.pages.len()).find(|p| {
                                            ed.page_doc(*p).comments.iter().any(|x| x.id == c.id)
                                        })
                                    {
                                        let _ = ed.with_page(p, |ed| {
                                            ed.resolve_comment(&c.id, !c.resolved)
                                        });
                                    }
                                    if target_name.is_some() && ui.small_button("Edit").clicked() {
                                        ed.focus_comment(&c.id);
                                        ed.comment_draft = Some(Draft {
                                            target: c.target,
                                            at: c.position(&ed.doc),
                                            text: c.body.clone(),
                                            editing: Some(c.id.clone()),
                                            focus: true,
                                        });
                                    }
                                    if ui.small_button("Delete").clicked()
                                        && let Some(p) = (0..ed.pages.len()).find(|p| {
                                            ed.page_doc(*p).comments.iter().any(|x| x.id == c.id)
                                        })
                                    {
                                        let _ = ed.with_page(p, |ed| ed.delete_comment(&c.id));
                                    }
                                });
                            });
                        });
                });
                ui.add_space(4.0);
                ui.separator();
            }
        });
}

pub fn overlay(ctx: &egui::Context, ed: &mut Editor, st: &mut CommentsUi, canvas: egui::Rect) {
    if ed.tool != Tool::Comment && !st.open && ed.comment_draft.is_none() {
        return;
    }
    let accent = theme::MAC_BLUE;
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            "comment-highlights".into(),
        ))
        .with_clip_rect(canvas);
    let target = ed
        .comment_draft
        .as_ref()
        .map(|d| d.target)
        .or(ed.hover)
        .or_else(|| {
            ed.focused_comment
                .as_ref()
                .and_then(|id| ed.doc.comments.iter().find(|c| &c.id == id))
                .map(|c| c.target)
        });
    if let Some(target) = target
        && let Some(r) = ed.doc.world_rect(target)
    {
        let r = ed.camera.world_rect_to_screen(&r);
        let rect = egui::Rect::from_min_size(
            canvas.min + egui::vec2(r.x as f32, r.y as f32),
            egui::vec2(r.w as f32, r.h as f32),
        );
        painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(0, 122, 255, 18));
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.5, accent),
            egui::StrokeKind::Inside,
        );
    }
    if ed.tool == Tool::Comment
        && ed.comment_draft.is_none()
        && let Some(at) = ctx.pointer_hover_pos().filter(|p| canvas.contains(*p))
    {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        let icon = egui::Rect::from_min_size(at + egui::vec2(12.0, 12.0), egui::vec2(18.0, 18.0));
        painter.rect_filled(icon.expand(3.0), 6.0, accent);
        theme::paint_icon(&painter, icon, Icon::Comment, Color32::WHITE);
    }
    let pins: Vec<_> = ed
        .doc
        .comments
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            (st.show_resolved || !c.resolved) && ed.doc.is_effectively_visible(c.target)
        })
        .map(|(i, c)| (i + 1, c.id.clone(), c.position(&ed.doc), c.resolved))
        .collect();
    for (number, id, at, resolved) in pins {
        let at = ed.camera.world_to_screen(at);
        let pos = canvas.min + egui::vec2(at.x as f32, at.y as f32);
        if !canvas.shrink(12.0).contains(pos) {
            continue;
        }
        egui::Area::new(egui::Id::new(("comment-pin", &id)))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fixed_pos(pos - egui::vec2(12.0, 12.0))
            .show(ctx, |ui| {
                let (r, response) =
                    ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                ui.painter().circle_filled(
                    r.center(),
                    12.0,
                    if resolved { Color32::GRAY } else { accent },
                );
                ui.painter()
                    .circle_stroke(r.center(), 12.0, Stroke::new(2.0, Color32::WHITE));
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    number.to_string(),
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                if response.on_hover_text("Open comment").clicked() {
                    st.open = true;
                    ed.focus_comment(&id);
                }
            });
    }
    if ed.comment_draft.is_none() {
        return;
    }
    let at = ed.comment_draft.as_ref().unwrap().at;
    let screen = ed.camera.world_to_screen(at);
    let pos = canvas.min + egui::vec2(screen.x as f32 + 18.0, screen.y as f32 + 18.0);
    let pos = egui::pos2(
        pos.x.clamp(
            canvas.left() + 8.0,
            (canvas.right() - 330.0).max(canvas.left() + 8.0),
        ),
        pos.y.clamp(
            canvas.top() + 8.0,
            (canvas.bottom() - 260.0).max(canvas.top() + 8.0),
        ),
    );
    let mut post = false;
    let mut cancel = false;
    let mut target_change = None;
    egui::Area::new("comment-composer".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .default_size(egui::vec2(322.0, 240.0))
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(16.0)
                .corner_radius(12.0)
                .show(ui, |ui| {
                    ui.set_width(290.0);
                    let d = ed.comment_draft.as_mut().unwrap();
                    let valid = ed.doc.get(d.target).is_some();
                    ui.label(
                        RichText::new(if d.editing.is_some() {
                            "Edit comment"
                        } else {
                            "Add comment"
                        })
                        .strong(),
                    );
                    let mut ancestors = ed.doc.ancestors(d.target);
                    ancestors.reverse();
                    ancestors.push(d.target);
                    ui.horizontal_wrapped(|ui| {
                        for id in ancestors {
                            if let Some(n) = ed.doc.get(id)
                                && ui
                                    .small_button(&n.name)
                                    .on_hover_text("Reference this element")
                                    .clicked()
                            {
                                target_change = Some(id);
                            }
                        }
                    });
                    ui.add_space(8.0);
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut d.text)
                            .id_salt("comment-body")
                            .hint_text("What would you like to change?")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                    if d.focus && ui.is_visible() {
                        response.request_focus();
                        d.focus = false;
                    }
                    if !valid {
                        ui.colored_label(
                            Color32::from_rgb(220, 80, 60),
                            "Element removed. Cancel this draft.",
                        );
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        post = ui
                            .add_enabled(
                                valid && !d.text.trim().is_empty(),
                                egui::Button::new(if d.editing.is_some() {
                                    "Save"
                                } else {
                                    "Post comment"
                                }),
                            )
                            .clicked();
                        ui.label(RichText::new("⌘ Enter").small().weak());
                    });
                    if valid
                        && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))
                    {
                        post = true;
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                });
        });
    if let Some(target) = target_change
        && let Some(r) = ed.doc.world_rect(target)
    {
        let d = ed.comment_draft.as_mut().unwrap();
        if d.editing.is_none() {
            d.target = target;
            d.at = v2(d.at.x.clamp(r.x, r.right()), d.at.y.clamp(r.y, r.bottom()));
        }
    }
    if cancel {
        ed.comment_draft = None;
        ed.revision += 1;
    } else if post && let Err(error) = ed.post_comment() {
        ed.status = error;
    }
}
