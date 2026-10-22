# Bind another project

The source workflow is generic. Keep project-specific paths and screen states in the binding, outside `SKILL.md`.

Create `~/Documents/Sketch-design/projects/<Sketch-project-id>.json` from the open project's `get_basic_info`. Use the actual project path as well as ID. A minimal binding has:

```json
{
  "version": 1,
  "projectId": "<open project ID>",
  "projectPath": "/absolute/path/to/Sketch-project",
  "stateDirectory": "/absolute/path/to/sync-state",
  "worktreeDirectory": "/absolute/path/to/design-feedback-worktrees",
  "source": {
    "repository": "/absolute/path/to/repository",
    "branch": "<existing working branch>",
    "inputPaths": ["src", "static", "package.json", "package-lock.json", "vite.config.ts"],
    "appDirectory": ".",
    "previewCwd": ".",
    "previewUrl": "http://127.0.0.1:8765/",
    "browserChannel": "chrome",
    "requiredChecks": [["npm", "test"]]
  },
  "screens": [
    {
      "key": "overview",
      "name": "Overview",
      "pageId": "<existing Sketch page ID>",
      "left": 0,
      "url": "/#overview",
      "selector": "main",
      "viewport": {"width": 1440, "height": 1000},
      "sourceFiles": ["src/Overview.svelte"],
      "steps": [{"role": "button", "name": "Pause", "optional": true}]
    }
  ]
}
```

`inputPaths` must include all source/configuration files that affect these screens and all files that code batches may edit. Untracked source files are included; symlinks in those paths are rejected. Ignored credentials, production configuration and unrelated directories must stay out. Changes to HEAD or any included file invalidate capture/check/publish versions.

For a static build, replace `previewUrl` with `previewFile`, a repository-relative HTML path. Include that file in `inputPaths` and rebuild it from the selected source. For HTTP, `previewCwd` must match the actual listener process's working directory relative to the chosen checkout. An incorrect process/port stops refresh. Source networking is limited to localhost and the CLI does not start servers.

The selected app needs Playwright installed. `appDirectory` selects its package context. An optional absolute `playwrightModule` can point to an existing installed Playwright entry point. Use an installed Chrome channel or a locally installed Playwright browser. The runtime uses Node.js and Python 3; conflict-preserving source exchange supports macOS and Linux and refuses unsupported platforms.

`initialScreens` is optional. To preserve existing imported artboards, provide a dictionary keyed by screen key with `key`, `name`, `nodeId`, `pageId`, `mapping:{}`, `sourceFiles`, and `source:{head:"<captured commit>",fingerprint:null}`. Pins on legacy imports without source identity stay on those artboards until verified explicitly. Without `initialScreens`, the first refresh creates new artboards and leaves preexisting design content untouched.

Runs contain browser captures, imported mappings and a complete Sketch project backup. Batches contain the source baseline, isolated worktree, check logs, exact source preimages/conflict versions and completion record. Do not delete these during recovery. A stale `operation.lock` records its PID; verify that operation is gone before removing only that lock.

A refresh can retain a pin for a removed/changed element. For an intentional verified replacement, `map` requires a completed source publish, current refresh, the initial unchanged comment, a replacement in the same latest screen and an existing verification artifact. It does not change the comment body. `complete` closes only explicitly named, unchanged batch items after refresh.
