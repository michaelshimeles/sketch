---
name: sketch-design
description: Use the local open-source Sketch app to design screens or review batches of pinned feedback. Refresh source-bound projects from Firstmate's latest working checkout while preserving pins, then apply clear comment batches to verified application code. Supports status, refresh, review, apply, design, and new projects.
---

# Sketch design

Work in the open local Sketch project. Claude Code and Codex share `http://127.0.0.1:29980/mcp`; keep `/Users/blie/Applications/Sketch.app` running and use one writer at a time.

Read `get_guide({topic:"sketch-rs-mcp-instructions"})`, `get_basic_info`, and `get_selection`. Retain the project's path and page IDs. Pass `fileId` as the exact project path on MCP reads and writes, so switching projects stops work. A pending comment draft must be posted or canceled before refresh.

## Commands

- `/sketch-design status`: report the project, selection, connected source checkout, captured version, and whether screens are current. Do not edit.
- `/sketch-design refresh`: capture the bound source's latest working screens, preserve comments, and report pins retained on previous screens.
- `/sketch-design review`: read all open comments and their source context. Report clear changes, questions, and stale targets without editing code. Report if refresh is needed.
- `/sketch-design apply`: for a source-bound project, refresh, apply clear feedback to code in an isolated worktree, verify, publish to the unchanged source, refresh Sketch, and resolve completed comments. This invocation authorizes those local code and design edits. It does not authorize deployment or upstream publication.
- `/sketch-design design <request>`: edit only the Sketch design.
- `/sketch-design <request>`: follow the requested scope. A direct source-code request uses the source workflow; a design request changes Sketch.

With no argument, use `refresh` then `review` for a bound project, or `review` for an unbound design. Do not ask for another approval for clear changes authorized by `apply`. Leave unclear or conflicting comments open while progressing with independent items.

## Source binding

The reusable CLI is `node <this-skill>/scripts/sketch-sync.mjs`. Resolve `<this-skill>` to the actual folder containing this file. It discovers the binding from the open project's ID under `~/Documents/Sketch-design/projects/<project-id>.json`. It works with future projects too; do not assume AEGIS.

Run `status` first. Missing binding means design-only mode. If the user requests source sync for an unbound project, read `references/source-sync.md`, identify the actual repository/working branch and local preview, and create a binding. Do not guess which repository supplies a design.

The binding selects the worker branch's registered Git worktree, including tracked and untracked source contents. Once that branch is clean and merged into main, use main. Every refresh verifies the preview process's working directory and fingerprints the chosen source before and after capture. If Firstmate moves to a new branch or preview, inspect its current Git worktrees and, if needed, its recorded history to update the binding. Do not reuse a preview belonging to another checkout.

The CLI starts no web server. Reuse the verified localhost preview. If it is absent, read the repository's instructions, build from the selected checkout, announce the precise port, directory, localhost address and stop method, then start only an allowed local preview. Static file previews are supported with `source.previewFile`; build them from the selected source before capturing. Never connect production sources to obtain design data.

## Refresh and pinned feedback

Run `refresh`. The CLI saves a project backup, captures each configured screen, imports editable layers with source identity mappings, and retains previous artboards below the current ones. Never replace or delete a previous artboard that holds feedback.

A pin moves automatically only when the same source selector and conservative semantic signature identify exactly one new element. Its ID, body, status, creation time and relative position remain intact. Ambiguous, removed, or older unmapped targets stay on their original reference screens. Report those as retained, not lost or successfully transferred. Refresh is a snapshot operation, not a continuous watcher.

Read native comments with `list_comments({fileId,status:"open",limit:100,offset:0})`; paginate until `total` is covered. `get_comment` supplies the page, element name/text, ancestors, bounds, click position and timestamps. The GUI uses `C`, hover highlight, click, type, **Post comment**. Sticky notes and TODO labels are unnecessary.

## Apply a batch to code

1. Run `prepare`. It refreshes first, snapshots the open comment batch and source fingerprint, and creates an isolated source worktree. Read the returned `batchFile` and `worktree`. New comments belong to the next batch.
2. Read that worktree's `AGENTS.md`, README, architecture and nearest tests. If `.codegraph/` exists, use CodeGraph before code searches. Source bindings list candidate component files and captured selectors/text, not guaranteed source line numbers. Read the current code and compare it with the captured element before editing.
3. Implement clear requests in the returned worktree. Keep these edits uncommitted until publishing; `git diff` shows changes against the staged starting source. Keep existing source edits from Firstmate, evidence labels, uncertainty and timestamps. Leave questions such as "what is this?" open unless they imply a clear change. For changed Svelte files, use the official Svelte MCP and run its autofixer. Install dependencies in this isolated worktree if needed; do not modify the source checkout's dependencies.
4. Run applicable checks through `verify <batchFile> -- <executable> <arguments...>`. All configured required checks must pass against the current worktree contents. Read failed logs and fix failures. Inspect the diff for unrelated edits. Run repository-required checks; state any local server details before checks that start one.
5. Run `publish <batchFile>`. It rejects a changed Firstmate baseline or stale checks. Source writes retain exact preimages and conflict versions. On a concurrent-edit error, stop publishing, retain the worktree and recovery records, and leave comments open. Refresh/reprepare against the newer code and reapply only this batch's clear changes. Do not force an old patch onto newer work, reset the worker, deploy, push or merge remotely.
6. Run `refresh` against the published working source. Inspect browser and Sketch screenshots of changed screens and confirm the requested behavior. Check spacing, clipping, contrast and phone layout where relevant. Do not resolve comments just because a code check passed.
7. If the requested change intentionally changed an element's text or structure and automatic pin transfer was uncertain, inspect the saved original and new mapping. After verifying the exact replacement, run `map <batchFile> <commentId> <newNodeId> <verification-artifact-path>`. This preserves feedback and records an explicit, batch-bound transfer. Never use a nearby or similarly named item as a substitute without checking.
8. Run `complete <batchFile> <completed-comment-ids...>` only for fully applied and visually verified items. It requires the published version to have been refreshed and accepts only unchanged feedback from the initial batch, allowing recorded anchor transfers. Concurrently edited comments remain open. Do not delete or rewrite user feedback.

Report the source checkout and version, completed items, remaining questions/retained pins, and any conflict or verification limit. Use page/element names, not raw internal IDs. Code changes remain local until the user requests normal publication through their repository workflow.

## Unbound designs and design-only edits

Reuse the design system. Read hierarchy, exact text and computed styles; prefer targeted updates and preserve element IDs. Check fonts and screenshots, fix observed issues, then call `finish_working_on_nodes`. Native tools include `add_comment`, `get_comment`, `update_comment`, `set_comment_status`, `delete_comment`, and `reanchor_comment`; discover the running schemas. Do not rewrite/delete feedback unless asked. A design-only edit does not update code or authorize deployment.

If MCP or the comment tools are unavailable, reconnect `sketch` through `/mcp` or start a fresh agent session. Say what is missing; do not silently substitute unanchored notes.
