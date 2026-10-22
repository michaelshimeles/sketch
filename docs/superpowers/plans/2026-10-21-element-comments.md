# Element comments implementation plan

> For agentic workers: use superpowers:executing-plans to implement task by task. The user approved building this design; execute natively in this session, then obtain a fresh whole-branch review.

**Goal:** Add element-pinned feedback to the native Sketch app and its MCP connection.

**Architecture:** Comments are page metadata in Document and participate in its transactions, persistence, and history. Editor operations own validation and mutations. egui paints comment overlays and the review panel; MCP calls the same operations.

**Tech Stack:** Rust 1.96.1 selected explicitly, existing egui/winit/wgpu, serde, rmcp. No new dependencies.

**Spec:** /Users/blie/Documents/Sketch-design/comment-mode-design.md

## Global constraints

- Use the existing native Rust application and the current project's pages.
- Keep comments separate from nodes and design exports; preserve existing project contents.
- Support single-message comments, open/resolved status, stable anchors, autosave, undo/redo, and MCP batching.
- Use a custom local build with the original app retained and no automatic upstream replacement.
- No upstream publication, release, deployment, or production changes.

## Review focus

- Hidden/clipped targets must not capture comments; locked visible targets must.
- Drafts survive pan/zoom and cannot be silently replaced by another target or page.
- Removed targets remain identifiable and undo restores their anchors.
- Comment bodies, IDs, offsets, and pagination reject malformed inputs without partial writes.
- Existing projects open and save safely; older builds reject the new page version.

### Task 1: Comment model, transactions, persistence, and MCP

Files: create `src/model/comment.rs`, `src/editor/comments.rs`, `tests/comments.rs`; modify document/history/project format/legacy file/page duplication and MCP dispatch/schema.

Interfaces: `Comment` stores ID, target NodeId, relative V2, fallback V2, target label, body, timestamps, resolved flag. `Editor::add_comment(target, point, body) -> Result<String,String>`, `update_comment(id, body)`, `resolve_comment(id, bool)`, `delete_comment(id)`; mutations use `Tx::comments_mut()`. `Comment::position(&Document) -> V2` follows live bounds. MCP exposes `list_comments`, `get_comment`, `add_comment`, `update_comment`, `delete_comment`, `set_comment_status` with full target context.

- [ ] Add integration tests that call the absent comment MCP tools and assert anchor movement, missing targets and undo, persisted status/body, cross-page pagination and clone independence, rejection of malformed inputs, and exclusion from SVG.
- [ ] Run `cargo +1.96.1 test --test comments` and record the expected unknown-tool failure.
- [ ] Implement the above model and shared editor operations. Add page format v4 with comments and migration from v3. Preserve comments in legacy save paths and page duplication with new IDs.
- [ ] Run `cargo +1.96.1 test --test comments`, expected all pass. Commit this deliverable.

### Task 2: Native cursor, composer, pins, and review panel

Files: modify editor input/tool/shortcuts, app input handling if necessary, theme icons, ui integration; create `src/ui/comments.rs` and editor comment input tests.

Interfaces: `Tool::Comment` uses C; editor stores pending anchor and focused comment; UI stores draft text/target and filter. `comments::panel` and `comments::overlay` consume the existing editor and canvas rect.

- [ ] Add scenario tests for C, locked/hidden/clipped target hit testing, no node motion, preserving a pending target on another click, and Escape behavior. Run tests and capture failure.
- [ ] Add the tool and hover highlighting; click creates a pending element anchor, not a design node. Add ancestor selection, Post/Cancel with Command+Enter/Escape, editable comment details, open/resolved list and numbered pins. Navigation retains drafts and prevents implicit switching.
- [ ] Test movement/resizing and pan/zoom in the native app, including canceled/empty drafts. Run `cargo +1.96.1 test --test comments` and appropriate workflow tests. Commit this deliverable.

### Task 3: Verify, install, and connect the generic skill

Files: update README and local `/sketch-design` skill; add ignored proof artifacts and local build packaging under Documents/Sketch-design.

- [ ] Run `cargo +1.96.1 fmt --check`, `cargo +1.96.1 clippy --all-targets -- -D warnings`, `cargo +1.96.1 test`, and `git diff --check`. Expected all pass; record any genuine environment limitations.
- [ ] Review the full branch with a fresh reviewer. Address findings and repeat affected checks.
- [ ] Back up the app and AEGIS project, install the custom app without Sparkle updates, and reopen the same project. Check UI interactions and MCP from an independent client; capture native screenshots. Remove only test-created comments.
- [ ] Update `/sketch-design` with the real comment tool schemas and instructions, package/validate it, and document the user flow. Commit source and retain the worktree for local maintenance.
