//! Tool catalogue: names, descriptions and input schemas.
//!
//! Names and argument shapes follow Paper's MCP server where Sketch has an equivalent
//! (`analysis/paper-mcp.md` §2), so agent prompts written for Paper carry over. Sketch
//! additions: `create_shape` (whiteboard shapes and arrows), `select_nodes`, `undo`, `redo`.

use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "\
Sketch is a local design and whiteboard app with an infinite canvas. These tools read and \
edit the project open in the running app; every change is one undo step and shows up live.

- Start with get_basic_info (pages, artboards, sizes) and get_selection (what the user is \
looking at). Use list_resources / open_file / create_file to pick a project.
- For details read get_guide({ topic: \"sketch-rs-mcp-instructions\" }) once per session.
- pageId defaults to the page the user is viewing; pass it to work on another page without \
switching the user's view. Node ids are opaque strings; never show them to the user.
- Build UI with create_artboard, then write_html into it, one visual group per call (a header, \
a row, a card) so the user sees progress. Inline styles only; flexbox (display:flex, gap, \
padding, align-items, justify-content) is the layout model; no margin, grid or tables.
- Text uses Inter unless font-family names a font installed on the system (check with \
get_font_family_info). Use px for font sizes.
- Edit with update_styles, set_text_content, rename_nodes, duplicate_nodes, move_nodes, \
delete_nodes. Whiteboard shapes, sticky notes and arrows: create_shape. Hand-drawn style: \
update_styles with {\"sketch\": true, \"roughness\": 1, \"fillStyle\": \"hachure\"}.
- Check your work with get_screenshot. Artboard height is a starting point: when content \
clips set height: \"fit-content\" with update_styles.
- Call finish_working_on_nodes when done.";

fn file_id() -> Value {
    json!({"type": "string", "description": "The project to operate on (id, name or folder path from list_resources). When omitted the project open in the app is used."})
}

fn page_id() -> Value {
    json!({"type": "string", "description": "The page to operate on. When omitted the page the user is viewing is used. The user's page can change between calls; prefer explicit page ids."})
}

fn styles_schema(desc: &str) -> Value {
    json!({
        "type": "object",
        "propertyNames": {"type": "string"},
        "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}, {"type": "boolean"}]},
        "description": desc,
    })
}

fn tool(name: &str, desc: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    let mut props = props;
    if let Some(m) = props.as_object_mut()
        && !m.contains_key("fileId")
        && !matches!(
            name,
            "list_resources" | "create_file" | "get_font_family_info" | "get_guide" | "open_file"
        )
    {
        m.insert("fileId".into(), file_id());
    }
    let ann = if read_only {
        json!({"readOnlyHint": true})
    } else if name == "delete_nodes" {
        json!({"destructiveHint": true, "readOnlyHint": false})
    } else {
        json!({"destructiveHint": false, "readOnlyHint": false})
    };
    json!({
        "name": name,
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        },
        "annotations": ann,
    })
}

/// Every tool, as MCP `Tool` JSON objects.
pub fn definitions() -> Vec<Value> {
    let node_id = |d: &str| json!({"type": "string", "description": d});
    let ids = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    vec![
        // ------------------------------------------------------------ projects and pages
        tool(
            "list_resources",
            "Lists the user's Sketch projects (local folders), most recently modified first. \
             Each item has id, name, path, page count, updatedAt, and `open` for the project open \
             in the app.",
            json!({"limit": {"type": "integer", "description": "Maximum projects to return. Defaults to 50."}}),
            &[],
            true,
        ),
        tool(
            "open_file",
            "Opens a project in the app (saving the current one) and returns a get_basic_info \
             response. Optional pageId switches the user to that page.",
            json!({
                "fileId": {"type": "string", "minLength": 1, "description": "Project id, name or folder path (see list_resources)."},
                "pageId": {"type": "string", "description": "Optional page to show."},
            }),
            &["fileId"],
            false,
        ),
        tool(
            "create_file",
            "Creates a new empty project and returns its id. Call open_file with the id to work in it.",
            json!({"name": {"type": "string", "description": "Optional project name."}}),
            &[],
            false,
        ),
        tool(
            "create_page",
            "Creates a new page at the end of the page list and returns its id, without switching \
             the user's page. Use the returned pageId in later calls.",
            json!({"name": {"type": "string", "description": "Optional page name. Defaults to \"Page N\"."}}),
            &[],
            false,
        ),
        tool(
            "rename_pages",
            "Rename one or more pages. Does not switch the user's page.",
            json!({"updates": {"type": "array", "description": "Page renames.", "items": {
                "type": "object",
                "properties": {"pageId": {"type": "string"}, "name": {"type": "string"}},
                "required": ["pageId", "name"],
            }}}),
            &["updates"],
            false,
        ),
        // ------------------------------------------------------------ reading
        tool(
            "get_basic_info",
            "Essential context about the open project: project name, the page, node count, \
             artboards (top-level frames) with ids, sizes and world positions, other top-level \
             layers, fonts, and `pages` with the active one marked isActive.\n\
             - worldX/worldY are world positions; x/y are relative to the parent.\n\
             - Pass pageId to inspect another page without switching the user's view.",
            json!({"pageId": page_id()}),
            &[],
            true,
        ),
        tool(
            "get_selection",
            "The selected layers on the page the user is viewing: ids, names, types, sizes, \
             positions and the artboard each belongs to.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_node_info",
            "Details of one node: type, name, size, visibility, lock state, parent, children ids, \
             text content, x/y (relative to parent) and worldX/worldY. Errors if it doesn't exist.",
            json!({"nodeId": node_id("The node to inspect.")}),
            &["nodeId"],
            true,
        ),
        tool(
            "get_children",
            "Direct children of a node (or of the page root when nodeId is \"root\"), with ids, \
             names, types, child counts and positions.",
            json!({"nodeId": node_id("The parent node, or \"root\"."), "pageId": page_id()}),
            &["nodeId"],
            true,
        ),
        tool(
            "get_tree_summary",
            "Compact indented text tree of a subtree: type, name, id and size per node. Much \
             cheaper than get_jsx for orientation. nodeId \"root\" summarizes the whole page.",
            json!({
                "nodeId": node_id("Root of the summary, or \"root\"."),
                "depth": {"type": "number", "description": "Maximum depth (default 3, max 10)."},
                "pageId": page_id(),
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "get_screenshot",
            "Render a node (or the whole page when nodeId is omitted or \"root\") to a PNG and \
             return it as an image. Scale 1 (default) is enough for layout checks; use 2 to read \
             small text. Large images are scaled down to fit API limits.",
            json!({
                "nodeId": node_id("The node to capture; omit for the whole page."),
                "scale": {"type": "number", "description": "Render scale: 1 (default) or 2."},
                "pageId": page_id(),
            }),
            &[],
            true,
        ),
        tool(
            "get_jsx",
            "JSX for a node and its descendants with inline styles (frames become flex divs, text \
             becomes spans, vectors become inline SVG). Use it to export a design to code.",
            json!({
                "nodeId": node_id("The node to convert."),
                "format": {"type": "string", "enum": ["inline-styles", "tailwind"], "description": "Only inline styles are produced; \"tailwind\" is accepted for compatibility."},
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "get_computed_styles",
            "CSS styles (camelCase) of one or more nodes, as {nodeId: styles}.",
            json!({"nodeIds": ids("Nodes to read.")}),
            &["nodeIds"],
            true,
        ),
        tool(
            "get_fill_image",
            "The image used by a node's image fill, as image content.",
            json!({"nodeId": node_id("A node with an image fill.")}),
            &["nodeId"],
            true,
        ),
        tool(
            "find_nodes",
            "Find nodes by text content and/or computed style. Searches the whole project by \
             default; pass pageId for one page or nodeId for a subtree. `textValue` matches text \
             case-insensitively with * wildcards anchored to the whole value. `filters` are \
             {styleName, styleValue} pairs (AND), values may use * wildcards; colors match by value.",
            json!({
                "pageId": page_id(),
                "nodeId": node_id("Optional subtree root; takes priority over pageId."),
                "textValue": {"type": "string"},
                "filters": {"type": "array", "items": {"type": "object", "properties": {
                    "styleName": {"type": "string"}, "styleValue": {"type": "string"}}}},
            }),
            &[],
            true,
        ),
        tool(
            "get_font_family_info",
            "Whether font families are installed and their weights/styles. Text renders in any \
             installed family; Inter (400 to 700) is bundled, and families that aren't \
             installed fall back to Inter.",
            json!({"familyNames": {"type": "array", "items": {"type": "string"}}}),
            &["familyNames"],
            true,
        ),
        tool(
            "get_guide",
            "Read a guide. Topics: \"sketch-rs-mcp-instructions\" (read first), \"html\" (what \
             write_html supports), \"whiteboard\" (shapes, arrows, stickies, hand-drawn style).",
            json!({"topic": {"type": "string"}}),
            &["topic"],
            true,
        ),
        tool(
            "export",
            "Export nodes to files. `nodes` maps node ids to export settings \
             ([{format: png|svg|pdf, scale: \"2x\" | \"512w\" | \"512h\" | \"720p\"}]; [] = 2x PNG). \
             Files go to `directory` (default: the Downloads folder). Returns the paths written.",
            json!({
                "nodes": {"type": "object", "additionalProperties": {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "format": {"type": "string", "enum": ["png", "svg", "pdf"]},
                        "scale": {"type": "string"},
                    },
                    "required": ["format"],
                }}},
                "directory": {"type": "string", "description": "Output folder (absolute path)."},
                "pageId": page_id(),
            }),
            &["nodes"],
            false,
        ),
        tool(
            "export_combined_pdf",
            "Export several nodes into one PDF, one page per node, ordered top-to-bottom then \
             left-to-right. Returns the file path.",
            json!({
                "nodeIds": ids("Nodes to put on the pages."),
                "path": {"type": "string", "description": "Output file (default: Downloads/<project>.pdf)."},
            }),
            &["nodeIds"],
            false,
        ),
        // ------------------------------------------------------------ writing
        tool(
            "create_artboard",
            "Creates an artboard (top-level frame) and returns its id; then add content with \
             write_html({mode: \"insert-children\"}). `styles` are camelCase CSS; width and height \
             are required (e.g. \"1440px\"). Artboards default to display:flex, \
             flexDirection:column and a white background, and are placed in an empty spot unless \
             left/top are given. Default sizes: desktop 1440×900, tablet 768×1024, mobile 390×844.",
            json!({
                "name": {"type": "string", "description": "Layer name."},
                "styles": {
                    "type": "object",
                    "properties": {"width": {"type": "string"}, "height": {"type": "string"}},
                    "required": ["width", "height"],
                    "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}]},
                    "description": "CSS styles, e.g. {\"width\": \"390px\", \"height\": \"844px\", \"backgroundColor\": \"#f5f5f5\", \"padding\": \"24px\", \"gap\": \"16px\"}",
                },
                "pageId": page_id(),
            }),
            &["name", "styles"],
            false,
        ),
        tool(
            "write_html",
            "Parse HTML with inline styles into layers. Write incrementally: each call should add \
             one visual group (a header, one list row, a card) so the user sees progress.\n\
             - mode \"insert-children\" appends the HTML inside targetNodeId (\"root\" = the page); \
             \"replace\" replaces targetNodeId with it.\n\
             - Inline styles only. Flexbox (display:flex, flex-direction, gap, padding, \
             align-items, justify-content, flex-grow, flex-wrap) is the layout model; block \
             elements stack vertically. position:absolute with left/top is supported. No margin, \
             grid or tables.\n\
             - Text: font-family (any installed font; missing ones fall back to Inter), font-size, \
             font-weight, font-style, color, line-height, letter-spacing, text-align, \
             text-decoration; <b>/<i>/<u>/<s>/<span style> make rich text.\n\
             - Boxes: background(-color/-image: colors, linear/radial gradients, url()), border, \
             border-radius, opacity, overflow:hidden, mix-blend-mode, width/height (px, %, \
             fit-content).\n\
             - <img src> takes paper-asset:///abs/path, file:// URLs, absolute paths or data: URIs. \
             <svg> markup is imported as vector layers.\n\
             - layer-name=\"...\" names layers; <x-sketch-clone node-id=\"ID\" style=\"...\"/> \
             clones an existing node.\n\
             Returns the ids of the created top-level layers and the styles that were ignored.",
            json!({
                "html": {"type": "string", "description": "HTML to parse into layers."},
                "targetNodeId": {"type": "string", "description": "Target node id, or \"root\" for the page."},
                "mode": {"type": "string", "enum": ["insert-children", "replace"]},
                "pageId": page_id(),
            }),
            &["html", "targetNodeId", "mode"],
            false,
        ),
        tool(
            "create_shape",
            "Create a whiteboard element: rectangle, ellipse, diamond, triangle, polygon, star, \
             line, arrow, sticky (sticky note with text) or text. Coordinates are world units, or \
             relative to parentId when given. Lines and arrows take `points` ([[x,y],...], at \
             least two) and arrows may bind to layers with startNodeId/endNodeId (the arrow then \
             follows them). `styles` takes the same CSS as update_styles plus the hand-drawn keys \
             sketch, roughness (0 architect, 1 artist, 2 cartoonist), fillStyle (hachure, \
             cross-hatch, solid, zigzag, dots), strokeStyle (solid, dashed, dotted). Arrows also \
             take route (straight, curved, elbow) and startHead/endHead (none, arrow, triangle, \
             dot, bar, diamond).",
            json!({
                "type": {"type": "string", "enum": ["rectangle", "ellipse", "diamond", "triangle", "polygon", "star", "line", "arrow", "sticky", "text"]},
                "x": {"type": "number"}, "y": {"type": "number"},
                "width": {"type": "number"}, "height": {"type": "number"},
                "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}},
                "text": {"type": "string", "description": "Text for sticky and text."},
                "name": {"type": "string"},
                "parentId": {"type": "string", "description": "Frame to place the shape in."},
                "startNodeId": {"type": "string"}, "endNodeId": {"type": "string"},
                "route": {"type": "string", "enum": ["straight", "curved", "elbow"]},
                "startHead": {"type": "string"}, "endHead": {"type": "string"},
                "sides": {"type": "integer", "description": "Polygon sides / star points."},
                "styles": styles_schema("CSS and hand-drawn styles."),
                "pageId": page_id(),
            }),
            &["type"],
            false,
        ),
        tool(
            "update_styles",
            "Update styles on one or more nodes (camelCase CSS, like React.CSSProperties). Setting \
             left/top on an artboard moves it. Hand-drawn keys: sketch, roughness, fillStyle, \
             strokeStyle, seed. Styles with no effect are returned under ignoredStyles.",
            json!({"updates": {"type": "array", "description": "Each item applies styles to nodeIds.", "items": {
                "type": "object",
                "properties": {
                    "nodeIds": {"type": "array", "items": {"type": "string"}},
                    "styles": styles_schema("CSS properties."),
                },
                "required": ["nodeIds", "styles"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "set_text_content",
            "Set the text of one or more Text or Sticky nodes (batch).",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {"nodeId": {"type": "string"}, "textContent": {"type": "string"}},
                "required": ["nodeId", "textContent"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "rename_nodes",
            "Rename layers (batch). Names are truncated to 50 characters.",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {"nodeId": {"type": "string"}, "name": {"type": "string"}},
                "required": ["nodeId", "name"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "delete_nodes",
            "Delete nodes and their descendants. Verify parents with get_node_info first when unsure.",
            json!({"nodeIds": ids("Nodes to delete.")}),
            &["nodeIds"],
            false,
        ),
        tool(
            "duplicate_nodes",
            "Deep-clone nodes. Each copy goes under parentId when given, else next to its source; \
             duplicated artboards are placed in an empty spot. Returns new ids and a \
             descendantIdMap from every original descendant id to its copy.",
            json!({"nodes": {"type": "array", "items": {
                "type": "object",
                "properties": {"id": {"type": "string"}, "parentId": {"type": "string"}},
                "required": ["id"],
            }}}),
            &["nodes"],
            false,
        ),
        tool(
            "move_nodes",
            "Move nodes keeping their ids. Each move is {nodeId, before: siblingId}, {nodeId, \
             after: siblingId} or {nodeId, parentId, index?} (parentId \"root\" = the page root). \
             Moves apply in order. In auto-layout parents this changes the visual order; in free \
             parents it changes stacking and keeps the world position.",
            json!({"moves": {"type": "array", "minItems": 1, "items": {
                "type": "object",
                "properties": {
                    "nodeId": {"type": "string"}, "before": {"type": "string"},
                    "after": {"type": "string"}, "parentId": {"type": "string"},
                    "index": {"type": "integer", "minimum": 0},
                },
                "required": ["nodeId"],
            }}}),
            &["moves"],
            false,
        ),
        tool(
            "select_nodes",
            "Select nodes on the page the user is viewing so they can see what you mean; \
             zoom: true also frames them in the viewport. An empty list clears the selection.",
            json!({"nodeIds": ids("Nodes to select."), "zoom": {"type": "boolean"}}),
            &["nodeIds"],
            false,
        ),
        tool(
            "undo",
            "Undo the last change on a page (the user's or an agent's).",
            json!({"pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "redo",
            "Redo the last undone change on a page.",
            json!({"pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "finish_working_on_nodes",
            "Call when done creating or editing. Saves the project.",
            json!({"nodeIds": ids("Optional nodes you worked on.")}),
            &[],
            false,
        ),
    ]
}

pub const GUIDE_INSTRUCTIONS: &str = "\
# Designing in Sketch over MCP

1. Orient: get_basic_info → pages, artboards, sizes; get_selection → what the user points at.
2. Plan a brief before a new design: palette (3–5 colors), type scale (e.g. 12/14/16/20/28/40),
   spacing scale (4/8/12/16/24/32), corner radius.
3. Create an artboard sized for the device (create_artboard). Artboards are flex columns.
4. Fill it with write_html, one visual group per call, into the artboard id. Keep the returned
   ids: later calls can target them (insert-children) or replace them.
5. Iterate with update_styles / set_text_content / duplicate_nodes instead of rewriting.
6. Review with get_screenshot (scale 1; 2 for small text). Fix clipping with
   update_styles({height: \"fit-content\"}) on the artboard.
7. finish_working_on_nodes at the end (also saves the project).

Layout model: every container is an auto-layout frame (flexbox). display:flex with
flex-direction row/column, gap, padding, justify-content (start, center, end, space-between),
align-items (start, center, end; stretch is the default and makes children fill the cross axis
when the container has a definite size). flex-grow:1 fills the main axis; width:100% fills too.
height/width fit-content hug the content. position:absolute + left/top places freely.

Whiteboard: create_shape makes rectangles, ellipses, diamonds, stickies, text and arrows that
bind to shapes (they re-route when shapes move). Turn on the hand-drawn look with
{sketch: true, roughness: 1, fillStyle: \"hachure\"}.

Node ids look like \"3fa9c1:14\" (page prefix and node number). Ids stay valid across calls.";

pub const GUIDE_HTML: &str = "\
# write_html reference

Elements: div/section/header/footer/nav/main/article/aside/ul/ol/li/button/form → frames;
p/h1–h6/span/label/a/strong/em/code/pre (elements holding only text) → text layers (with a
frame around them when they have a background, border or padding); img → image rectangle;
svg → vector layers; br → line break; x-sketch-clone node-id=ID → copy of an existing layer.

Default sizes (like CSS): h1 32px bold, h2 24px bold, h3 20px bold, h4–h6 16px bold, p 16px,
small 13px, code/pre 14px; color is inherited, default black.

Styles: width, height (px, %, fit-content), left/top/right/bottom with position:absolute,
display (flex, block, none), flex-direction, flex-wrap, gap, row-gap, column-gap, padding*,
justify-content, align-items, align-self, flex-grow, overflow, background, background-color,
background-image (url(), linear-gradient(), radial-gradient()), background-size/object-fit,
border, border-color/width/style, border-radius (1–4 values), opacity, mix-blend-mode, color,
font-family, font-size, font-weight, font-style, line-height, letter-spacing, text-align,
text-decoration. Text uses any installed font family and the nearest installed weight;
missing families fall back to Inter. Spans inherit the node's family.
Ignored (reported back): margin, grid, box-shadow, transform, filters, transitions.";

pub const GUIDE_WHITEBOARD: &str = "\
# Whiteboard elements

create_shape({type, x, y, width, height, styles}) for rectangle, ellipse, diamond, triangle,
polygon (sides), star (sides = points), sticky (text, background color), text (text).
Lines and arrows: create_shape({type: \"arrow\", points: [[x1,y1],[x2,y2]],
startNodeId, endNodeId, route: \"elbow\", endHead: \"triangle\"}). With start/end nodes the
points can be omitted: the arrow is routed between the shapes and follows them when they move.

Hand-drawn style (Excalidraw-like): styles {sketch: true, roughness: 0|1|2,
fillStyle: hachure|cross-hatch|solid|zigzag|dots, strokeStyle: solid|dashed|dotted, seed}.
Colors: backgroundColor (fill), borderColor/stroke, borderWidth/strokeWidth.";
