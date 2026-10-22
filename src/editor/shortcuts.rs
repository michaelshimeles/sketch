//! Keyboard shortcuts. One table drives dispatch, the in-app help and the README.

use super::Tool;
use super::input::{Key, Modifiers};
use super::vector_ops::BoolOp;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Tool(Tool),
    Escape,
    EditText,
    Delete,
    Undo,
    Redo,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Duplicate,
    Forward,
    Backward,
    ToFront,
    ToBack,
    ZoomIn,
    ZoomOut,
    Zoom100,
    ZoomFit,
    ZoomSelection,
    ToggleVisible,
    ToggleLock,
    Nudge(f64, f64),
    WrapInFrame,
    New,
    Open,
    Save,
    SaveAs,
    ExportPng,
    ExportSvg,
    ToggleSnapping,
    Group,
    Ungroup,
    CreateComponent,
    DetachInstance,
    AddAutoLayout,
    RemoveAutoLayout,
    /// Axis (0 = x, 1 = y) and target (0 start, 0.5 center, 1 end).
    Align(u8, f64),
    Distribute(u8),
    Boolean(super::vector_ops::BoolOp),
    Flatten,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    PlaceImage,
    ToggleSketch,
    CommandPalette,
    Rename,
    ToggleUi,
    ToggleHelp,
    ToggleGrid,
    SelectNext,
    SelectPrev,
    SelectParent,
    NextPage,
    PrevPage,
    FlipH,
    FlipV,
    CopyProps,
    PasteProps,
    Home,
    ExportPdf,
    CopyAsPng,
    CopyAsSvg,
}

impl Action {
    pub fn repeatable(&self) -> bool {
        matches!(
            self,
            Action::Nudge(..) | Action::Undo | Action::Redo | Action::ZoomIn | Action::ZoomOut
        )
    }
}

/// `(cmd, shift, alt)` requirement for a binding. `cmd` is Cmd on macOS and Ctrl elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chord {
    cmd: bool,
    shift: bool,
    alt: bool,
}

const fn c(cmd: bool, shift: bool, alt: bool) -> Chord {
    Chord { cmd, shift, alt }
}
const PLAIN: Chord = c(false, false, false);
const SHIFT: Chord = c(false, true, false);
const CMD: Chord = c(true, false, false);
const CMD_SHIFT: Chord = c(true, true, false);
const CMD_ALT: Chord = c(true, false, true);
const ALT: Chord = c(false, false, true);
const ALT_SHIFT: Chord = c(false, true, true);
const CMD_ALT_SHIFT: Chord = c(true, true, true);

/// Bindings: (key, chord, action, description shown in help).
fn bindings() -> Vec<(Key, Chord, Action, &'static str)> {
    use Action as A;
    use Key::*;
    vec![
        (Char('c'), PLAIN, A::Tool(Tool::Comment), "Comment tool"),
        (Char('v'), PLAIN, A::Tool(Tool::Select), "Select tool"),
        (
            Char('h'),
            PLAIN,
            A::Tool(Tool::Hand),
            "Hand tool (or hold Space)",
        ),
        (Char('f'), PLAIN, A::Tool(Tool::Frame), "Frame tool"),
        (Char('a'), PLAIN, A::Tool(Tool::Frame), "Frame tool"),
        (Char('r'), PLAIN, A::Tool(Tool::Rect), "Rectangle tool"),
        (Char('o'), PLAIN, A::Tool(Tool::Ellipse), "Ellipse tool"),
        (Char('l'), PLAIN, A::Tool(Tool::Line), "Line tool"),
        (Char('t'), PLAIN, A::Tool(Tool::Text), "Text tool"),
        (Char('p'), PLAIN, A::Tool(Tool::Pen), "Pen (vector path)"),
        (
            Char('p'),
            SHIFT,
            A::Tool(Tool::Pencil),
            "Pencil (freehand sketch)",
        ),
        (Char('y'), SHIFT, A::Tool(Tool::Polygon), "Polygon tool"),
        (Char('d'), PLAIN, A::Tool(Tool::Diamond), "Diamond tool"),
        (Char('l'), SHIFT, A::Tool(Tool::Arrow), "Arrow tool"),
        (Char('s'), PLAIN, A::Tool(Tool::Sticky), "Sticky note"),
        (Char('k'), PLAIN, A::Tool(Tool::Laser), "Laser pointer"),
        (Char('1'), PLAIN, A::Tool(Tool::Select), "Select tool"),
        (Char('2'), PLAIN, A::Tool(Tool::Rect), "Rectangle tool"),
        (Char('3'), PLAIN, A::Tool(Tool::Diamond), "Diamond tool"),
        (Char('4'), PLAIN, A::Tool(Tool::Ellipse), "Ellipse tool"),
        (Char('5'), PLAIN, A::Tool(Tool::Arrow), "Arrow tool"),
        (Char('6'), PLAIN, A::Tool(Tool::Line), "Line tool"),
        (
            Char('7'),
            PLAIN,
            A::Tool(Tool::Pencil),
            "Pencil (freehand sketch)",
        ),
        (Char('8'), PLAIN, A::Tool(Tool::Text), "Text tool"),
        (Char('9'), PLAIN, A::PlaceImage, "Place image…"),
        (Char('0'), PLAIN, A::Tool(Tool::Eraser), "Eraser"),
        (
            Char('h'),
            CMD_ALT,
            A::ToggleSketch,
            "Hand-drawn style on/off",
        ),
        (Char('k'), CMD, A::CommandPalette, "Command palette"),
        (Char('p'), CMD_ALT, A::ExportPdf, "Export PDF"),
        (Char('c'), CMD_SHIFT, A::CopyAsPng, "Copy as PNG"),
        (Char('c'), CMD_ALT_SHIFT, A::CopyAsSvg, "Copy as SVG"),
        (Char('/'), CMD_SHIFT, A::CommandPalette, "Command palette"),
        (Char('r'), CMD, A::Rename, "Rename layer"),
        (Char('\\'), CMD, A::ToggleUi, "Show / hide UI"),
        (Char('/'), CMD, A::ToggleHelp, "Keyboard shortcuts"),
        (Char('?'), SHIFT, A::ToggleHelp, "Keyboard shortcuts"),
        (Char('\''), CMD_SHIFT, A::ToggleGrid, "Show / hide dot grid"),
        (Tab, PLAIN, A::SelectNext, "Select next sibling"),
        (Tab, SHIFT, A::SelectPrev, "Select previous sibling"),
        (Enter, SHIFT, A::SelectParent, "Select parent"),
        (PageDown, PLAIN, A::NextPage, "Next page"),
        (PageUp, PLAIN, A::PrevPage, "Previous page"),
        (Char('h'), SHIFT, A::FlipH, "Flip horizontal"),
        (Char('v'), SHIFT, A::FlipV, "Flip vertical"),
        (Char('c'), CMD_ALT, A::CopyProps, "Copy properties"),
        (Char('v'), CMD_ALT, A::PasteProps, "Paste properties"),
        (Char('0'), SHIFT, A::Zoom100, "Zoom to 100%"),
        (Char(')'), SHIFT, A::Zoom100, "Zoom to 100%"),
        (Char('h'), CMD_ALT_SHIFT, A::Home, "All projects"),
        (Char('s'), SHIFT, A::Tool(Tool::Star), "Star tool"),
        (
            Char('b'),
            PLAIN,
            A::Tool(Tool::Pencil),
            "Pencil (freehand sketch)",
        ),
        (Char('e'), PLAIN, A::Tool(Tool::Eraser), "Eraser"),
        (
            Escape,
            PLAIN,
            A::Escape,
            "Cancel / select parent / deselect",
        ),
        (Enter, PLAIN, A::EditText, "Edit text / select children"),
        (Delete, PLAIN, A::Delete, "Delete selection"),
        (Backspace, PLAIN, A::Delete, "Delete selection"),
        (Char('z'), CMD, A::Undo, "Undo"),
        (Char('z'), CMD_SHIFT, A::Redo, "Redo"),
        (Char('y'), CMD, A::Redo, "Redo"),
        (Char('a'), CMD, A::SelectAll, "Select all"),
        (Char('c'), CMD, A::Copy, "Copy"),
        (Char('x'), CMD, A::Cut, "Cut"),
        (Char('v'), CMD, A::Paste, "Paste"),
        (Char('d'), CMD, A::Duplicate, "Duplicate"),
        (Char(']'), CMD, A::Forward, "Bring forward"),
        (Char('['), CMD, A::Backward, "Send backward"),
        (Char(']'), CMD_SHIFT, A::ToFront, "Bring to front"),
        (Char('['), CMD_SHIFT, A::ToBack, "Send to back"),
        (Char('}'), CMD_SHIFT, A::ToFront, "Bring to front"),
        (Char('{'), CMD_SHIFT, A::ToBack, "Send to back"),
        (Char('='), CMD, A::ZoomIn, "Zoom in"),
        (Char('+'), CMD, A::ZoomIn, "Zoom in"),
        (Char('+'), CMD_SHIFT, A::ZoomIn, "Zoom in"),
        (Char('-'), CMD, A::ZoomOut, "Zoom out"),
        (Char('0'), CMD, A::Zoom100, "Zoom to 100%"),
        (Char('1'), SHIFT, A::ZoomFit, "Zoom to fit"),
        (Char('!'), SHIFT, A::ZoomFit, "Zoom to fit"),
        (Char('2'), SHIFT, A::ZoomSelection, "Zoom to selection"),
        (Char('@'), SHIFT, A::ZoomSelection, "Zoom to selection"),
        (Char('h'), CMD_SHIFT, A::ToggleVisible, "Show / hide"),
        (Char('l'), CMD_SHIFT, A::ToggleLock, "Lock / unlock"),
        (
            Char('g'),
            CMD_ALT,
            A::WrapInFrame,
            "Wrap selection in frame",
        ),
        (Left, PLAIN, A::Nudge(-1.0, 0.0), "Nudge left"),
        (Right, PLAIN, A::Nudge(1.0, 0.0), "Nudge right"),
        (Up, PLAIN, A::Nudge(0.0, -1.0), "Nudge up"),
        (Down, PLAIN, A::Nudge(0.0, 1.0), "Nudge down"),
        (Left, SHIFT, A::Nudge(-10.0, 0.0), "Nudge left 10"),
        (Right, SHIFT, A::Nudge(10.0, 0.0), "Nudge right 10"),
        (Up, SHIFT, A::Nudge(0.0, -10.0), "Nudge up 10"),
        (Down, SHIFT, A::Nudge(0.0, 10.0), "Nudge down 10"),
        (Char('n'), CMD, A::New, "New project"),
        (Char('o'), CMD, A::Open, "Import file…"),
        (Char('s'), CMD, A::Save, "Save now"),
        (Char('s'), CMD_SHIFT, A::SaveAs, "Duplicate project"),
        (Char('e'), CMD_SHIFT, A::ExportPng, "Export PNG"),
        (Char('e'), CMD_ALT_SHIFT, A::ExportSvg, "Export SVG"),
        (Char('g'), CMD, A::Group, "Group selection"),
        (Char('g'), CMD_SHIFT, A::Ungroup, "Ungroup"),
        (Char('k'), CMD_ALT, A::CreateComponent, "Create component"),
        (Char('b'), CMD_ALT, A::DetachInstance, "Detach instance"),
        (Char('a'), SHIFT, A::AddAutoLayout, "Add auto layout"),
        (
            Char('a'),
            ALT_SHIFT,
            A::RemoveAutoLayout,
            "Remove auto layout",
        ),
        (Char('a'), ALT, A::Align(0, 0.0), "Align left"),
        (Char('h'), ALT, A::Align(0, 0.5), "Align horizontal centers"),
        (Char('d'), ALT, A::Align(0, 1.0), "Align right"),
        (Char('w'), ALT, A::Align(1, 0.0), "Align top"),
        (Char('v'), ALT, A::Align(1, 0.5), "Align vertical centers"),
        (Char('s'), ALT, A::Align(1, 1.0), "Align bottom"),
        (
            Char('h'),
            ALT_SHIFT,
            A::Distribute(0),
            "Distribute horizontal spacing",
        ),
        (
            Char('v'),
            ALT_SHIFT,
            A::Distribute(1),
            "Distribute vertical spacing",
        ),
        (
            Char('u'),
            ALT_SHIFT,
            A::Boolean(BoolOp::Union),
            "Union selection",
        ),
        (
            Char('s'),
            ALT_SHIFT,
            A::Boolean(BoolOp::Subtract),
            "Subtract selection",
        ),
        (
            Char('i'),
            ALT_SHIFT,
            A::Boolean(BoolOp::Intersect),
            "Intersect selection",
        ),
        (
            Char('e'),
            ALT_SHIFT,
            A::Boolean(BoolOp::Exclude),
            "Exclude selection",
        ),
        (Char('e'), CMD, A::Flatten, "Flatten to vector"),
        (Char('b'), CMD, A::Bold, "Bold"),
        (Char('i'), CMD, A::Italic, "Italic"),
        (Char('u'), CMD, A::Underline, "Underline"),
        (Char('x'), CMD_SHIFT, A::Strikethrough, "Strikethrough"),
        (Char('k'), CMD_SHIFT, A::PlaceImage, "Place image…"),
        (Char('\''), CMD, A::ToggleSnapping, "Toggle snapping"),
    ]
}

/// Key plus whether ⌘/Ctrl, Shift and Alt are required.
pub type KeyChord = (Key, bool, bool, bool);

/// Every binding: chord, action and description.
pub fn all_bindings() -> Vec<(KeyChord, (), Action, &'static str)> {
    bindings()
        .into_iter()
        .map(|(k, c, a, d)| ((k, c.cmd, c.shift, c.alt), (), a, d))
        .collect()
}

pub fn lookup(key: &Key, m: Modifiers) -> Option<Action> {
    let key = match key {
        Key::Char(ch) => Key::Char(ch.to_ascii_lowercase()),
        k => k.clone(),
    };
    let chord = c(m.command(), m.shift, m.alt);
    bindings()
        .into_iter()
        .find(|(k, ch, _, _)| *k == key && *ch == chord)
        .map(|(_, _, a, _)| a)
}

/// Help description of the first binding for an action.
pub fn lookup_desc(action: Action) -> Option<&'static str> {
    bindings()
        .into_iter()
        .find(|(_, _, a, _)| *a == action)
        .map(|(_, _, _, d)| d)
}

/// Human-readable list for help and docs, deduplicated by description.
pub fn help_table() -> Vec<(String, &'static str)> {
    let cmd = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    let mut out: Vec<(String, &'static str)> = Vec::new();
    for (k, ch, _, desc) in bindings() {
        let key = match &k {
            Key::Char(c) => c.to_ascii_uppercase().to_string(),
            Key::Left => "←".into(),
            Key::Right => "→".into(),
            Key::Up => "↑".into(),
            Key::Down => "↓".into(),
            other => format!("{other:?}"),
        };
        let mut s = String::new();
        if ch.cmd {
            s += cmd;
        }
        if ch.alt {
            s += if cfg!(target_os = "macos") {
                "⌥"
            } else {
                "Alt+"
            };
        }
        if ch.shift {
            s += if cfg!(target_os = "macos") {
                "⇧"
            } else {
                "Shift+"
            };
        }
        s += &key;
        if let Some(e) = out.iter_mut().find(|(_, d)| *d == desc) {
            if !e.0.contains(&s)
                && e.0.matches(" / ").count() < 1
                && !matches!(k, Key::Char('!' | '@' | '{' | '}' | '+'))
            {
                e.0 = format!("{} / {}", e.0, s);
            }
        } else {
            out.push((s, desc));
        }
    }
    out
}

/// Help-panel section of an action.
pub fn category(a: Action) -> &'static str {
    use Action as A;
    match a {
        A::Tool(_) => "Tools",
        A::New
        | A::Open
        | A::Save
        | A::SaveAs
        | A::ExportPng
        | A::ExportSvg
        | A::PlaceImage
        | A::Home => "File",
        A::Undo
        | A::Redo
        | A::Copy
        | A::Cut
        | A::Paste
        | A::Duplicate
        | A::Delete
        | A::SelectAll
        | A::CopyProps
        | A::PasteProps
        | A::Rename
        | A::CommandPalette
        | A::Escape
        | A::EditText
        | A::SelectNext
        | A::SelectPrev
        | A::SelectParent => "Edit",
        A::ZoomIn
        | A::ZoomOut
        | A::Zoom100
        | A::ZoomFit
        | A::ZoomSelection
        | A::ToggleSnapping
        | A::ToggleGrid
        | A::ToggleUi
        | A::ToggleHelp
        | A::NextPage
        | A::PrevPage => "View",
        A::Bold | A::Italic | A::Underline | A::Strikethrough => "Text",
        A::Boolean(_) | A::Flatten | A::ToggleSketch => "Vector & style",
        _ => "Arrange & layout",
    }
}

/// Help entries grouped by section, in a stable order.
pub fn help_sections() -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let table = help_table();
    let order = [
        "Tools",
        "Edit",
        "Arrange & layout",
        "Vector & style",
        "Text",
        "View",
        "File",
    ];
    let binds = bindings();
    order
        .iter()
        .map(|cat| {
            let rows: Vec<(String, &'static str)> = table
                .iter()
                .filter(|(_, d)| {
                    binds
                        .iter()
                        .find(|(_, _, _, bd)| bd == d)
                        .is_some_and(|(_, _, a, _)| category(*a) == *cat)
                })
                .cloned()
                .collect();
            (*cat, rows)
        })
        .filter(|(_, r)| !r.is_empty())
        .collect()
}

/// One entry of the application menus (shared by the egui menu bar and the macOS menu).
pub enum MenuEntry {
    Item(&'static str, Action),
    Separator,
    Submenu(&'static str, Vec<MenuEntry>),
}

/// The first binding of an action: (key, cmd, shift, alt).
pub fn chord_of(a: Action) -> Option<(Key, bool, bool, bool)> {
    bindings()
        .into_iter()
        .find(|(_, _, x, _)| *x == a)
        .map(|(k, c, _, _)| (k, c.cmd, c.shift, c.alt))
}

/// The application menus.
pub fn menus() -> Vec<(&'static str, Vec<MenuEntry>)> {
    use super::vector_ops::BoolOp;
    use Action as A;
    use MenuEntry::{Item as I, Separator as S, Submenu as Sub};
    vec![
        (
            "File",
            vec![
                I("New Project", A::New),
                I("Import…", A::Open),
                I("Place Image…", A::PlaceImage),
                S,
                I("Save Now", A::Save),
                I("Duplicate Project", A::SaveAs),
                I("All Projects", A::Home),
                S,
                I("Export PNG…", A::ExportPng),
                I("Export SVG…", A::ExportSvg),
                I("Export PDF…", A::ExportPdf),
            ],
        ),
        (
            "Edit",
            vec![
                I("Undo", A::Undo),
                I("Redo", A::Redo),
                S,
                I("Cut", A::Cut),
                I("Copy", A::Copy),
                I("Copy as PNG", A::CopyAsPng),
                I("Copy as SVG", A::CopyAsSvg),
                I("Paste", A::Paste),
                I("Duplicate", A::Duplicate),
                I("Delete", A::Delete),
                I("Select All", A::SelectAll),
                S,
                I("Copy Properties", A::CopyProps),
                I("Paste Properties", A::PasteProps),
                S,
                I("Rename", A::Rename),
                I("Command Palette…", A::CommandPalette),
            ],
        ),
        (
            "Object",
            vec![
                I("Group Selection", A::Group),
                I("Ungroup", A::Ungroup),
                I("Frame Selection", A::WrapInFrame),
                S,
                I("Create Component", A::CreateComponent),
                I("Detach Instance", A::DetachInstance),
                S,
                I("Add Auto Layout", A::AddAutoLayout),
                I("Remove Auto Layout", A::RemoveAutoLayout),
                S,
                Sub(
                    "Align",
                    vec![
                        I("Align Left", A::Align(0, 0.0)),
                        I("Align Horizontal Centers", A::Align(0, 0.5)),
                        I("Align Right", A::Align(0, 1.0)),
                        S,
                        I("Align Top", A::Align(1, 0.0)),
                        I("Align Vertical Centers", A::Align(1, 0.5)),
                        I("Align Bottom", A::Align(1, 1.0)),
                        S,
                        I("Distribute Horizontal Spacing", A::Distribute(0)),
                        I("Distribute Vertical Spacing", A::Distribute(1)),
                    ],
                ),
                Sub(
                    "Arrange",
                    vec![
                        I("Bring Forward", A::Forward),
                        I("Send Backward", A::Backward),
                        I("Bring to Front", A::ToFront),
                        I("Send to Back", A::ToBack),
                    ],
                ),
                S,
                I("Flip Horizontal", A::FlipH),
                I("Flip Vertical", A::FlipV),
                S,
                I("Show/Hide", A::ToggleVisible),
                I("Lock/Unlock", A::ToggleLock),
            ],
        ),
        (
            "Vector",
            vec![
                I("Union", A::Boolean(BoolOp::Union)),
                I("Subtract", A::Boolean(BoolOp::Subtract)),
                I("Intersect", A::Boolean(BoolOp::Intersect)),
                I("Exclude", A::Boolean(BoolOp::Exclude)),
                S,
                I("Flatten", A::Flatten),
                S,
                I("Hand-drawn Style", A::ToggleSketch),
            ],
        ),
        (
            "Text",
            vec![
                I("Bold", A::Bold),
                I("Italic", A::Italic),
                I("Underline", A::Underline),
                I("Strikethrough", A::Strikethrough),
            ],
        ),
        (
            "View",
            vec![
                I("Zoom In", A::ZoomIn),
                I("Zoom Out", A::ZoomOut),
                I("Zoom to 100%", A::Zoom100),
                I("Zoom to Fit", A::ZoomFit),
                I("Zoom to Selection", A::ZoomSelection),
                S,
                I("Next Page", A::NextPage),
                I("Previous Page", A::PrevPage),
                S,
                I("Snapping", A::ToggleSnapping),
                I("Dot Grid", A::ToggleGrid),
                I("Show/Hide UI", A::ToggleUi),
                I("Keyboard Shortcuts", A::ToggleHelp),
            ],
        ),
        (
            "Tools",
            super::Tool::ALL
                .iter()
                .map(|t| I(t.label(), A::Tool(*t)))
                .collect(),
        ),
    ]
}

/// Human-readable chord text for menus ("⌘⇧E", "Ctrl+Shift+E", "D").
pub fn chord_text(a: Action) -> String {
    let Some((k, cmd, shift, alt)) = chord_of(a) else {
        return String::new();
    };
    let mac = cfg!(target_os = "macos");
    let mut s = String::new();
    if cmd {
        s += if mac { "⌘" } else { "Ctrl+" };
    }
    if alt {
        s += if mac { "⌥" } else { "Alt+" };
    }
    if shift {
        s += if mac { "⇧" } else { "Shift+" };
    }
    s += &match k {
        Key::Char(c) => c.to_ascii_uppercase().to_string(),
        Key::Left => "←".into(),
        Key::Right => "→".into(),
        Key::Up => "↑".into(),
        Key::Down => "↓".into(),
        other => format!("{other:?}"),
    };
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_menu_action_has_a_binding_or_is_menu_only() {
        for (_, items) in super::menus() {
            fn walk(e: &[super::MenuEntry]) {
                for x in e {
                    match x {
                        super::MenuEntry::Item(label, a) => {
                            // Delete is bound to the Delete key; everything should be reachable.
                            assert!(
                                super::chord_of(*a).is_some()
                                    || matches!(a, super::Action::ExportSvg),
                                "{label} has no shortcut"
                            );
                        }
                        super::MenuEntry::Submenu(_, v) => walk(v),
                        super::MenuEntry::Separator => {}
                    }
                }
            }
            walk(&items);
        }
        assert!(super::help_sections().len() >= 6);
    }

    use super::*;

    #[test]
    fn lookup_respects_modifiers() {
        assert_eq!(
            lookup(&Key::Char('z'), Modifiers::cmd()),
            Some(Action::Undo)
        );
        assert_eq!(
            lookup(&Key::Char('Z'), Modifiers::cmd_shift()),
            Some(Action::Redo)
        );
        assert_eq!(
            lookup(&Key::Char('r'), Modifiers::NONE),
            Some(Action::Tool(Tool::Rect))
        );
        assert_eq!(lookup(&Key::Char('r'), Modifiers::SHIFT), None);
        assert_eq!(
            lookup(&Key::Char('a'), Modifiers::cmd()),
            Some(Action::SelectAll)
        );
    }

    #[test]
    fn no_conflicting_bindings() {
        let b = bindings();
        for (i, x) in b.iter().enumerate() {
            for y in &b[i + 1..] {
                assert!(!(x.0 == y.0 && x.1 == y.1), "conflict: {:?} {:?}", x.0, x.1);
            }
        }
        assert!(help_table().len() > 30);
    }
}
