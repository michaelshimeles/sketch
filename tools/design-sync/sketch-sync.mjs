#!/usr/bin/env node
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { createRequire } from 'node:module';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { call, payload } from './client.mjs';
import { readJson, writeJson, git, sourceRoot, sourceSnapshot, assertSource, safeMatch,
  digest, feedbackStillCurrent, copySource, publishFiles, verifyCommand } from './core.mjs';
import { normalizeHtml } from './html.mjs';

let projectPath;
const guardedArgs = args => projectPath ? { ...args, fileId: projectPath } : args;
const api = async (name, args = {}) => payload(await call(name, guardedArgs(args)));
const stamp = () => new Date().toISOString().replace(/[:.]/g, '-');
const here = path.dirname(fileURLToPath(import.meta.url));
async function optionalJson(file, otherwise) {
  try { return await readJson(file); } catch (e) { if (e.code === 'ENOENT') return otherwise; throw e; }
}
async function allComments() {
  const out = [];
  for (let offset = 0; ; offset += 500) {
    const result = await api('list_comments', { status: 'all', limit: 500, offset });
    out.push(...result.comments);
    if (out.length >= result.total) return out;
  }
}
export function mappingFor(comment, state) {
  for (const screen of [...Object.values(state.screens || {}), ...(state.history || [])]) {
    const anchor = screen.mapping?.[comment.nodeId];
    if (anchor) return { screen: screen.key, source: screen.source, sourceFiles: screen.sourceFiles,
      captured: anchor, current: state.screens[screen.key]?.mapping?.[comment.nodeId] || safeMatch(anchor, Object.values(state.screens[screen.key]?.mapping || {})) };
    if (screen.nodeId === comment.nodeId) return { screen: screen.key, source: screen.source,
      sourceFiles: screen.sourceFiles, captured: { key: '__artboard', signature: screen.key, label: screen.name },
      current: { nodeId: state.screens[screen.key].nodeId } };
  }
  return null;
}
async function checkProject(config) {
  projectPath = config.projectPath;
  const info = await api('get_basic_info');
  if (info.fileId !== config.projectId || path.resolve(info.path) !== path.resolve(config.projectPath))
    throw new Error('The open Sketch project does not match this source binding. Open the intended project first.');
  if (info.commentDraftPending) throw new Error('Post or cancel your draft in Sketch before syncing.');
  return info;
}
function checkPreview(root, config) {
  if (config.source.previewFile) {
    const file = path.resolve(root, config.source.previewFile);
    if (!file.startsWith(path.resolve(root) + path.sep)) throw new Error('Local preview file escaped the chosen source.');
    return;
  }
  const url = new URL(config.source.previewUrl);
  if (!['127.0.0.1', 'localhost'].includes(url.hostname) || url.protocol !== 'http:')
    throw new Error('A sync preview must use HTTP on localhost.');
  const pids = execFileSync('lsof', ['-t', '-nP', `-iTCP:${url.port || 80}`, '-sTCP:LISTEN'], { encoding: 'utf8' }).trim().split('\n');
  const wanted = path.resolve(root, config.source.previewCwd || '.');
  for (const pid of pids) {
    const output = execFileSync('lsof', ['-a', '-p', pid, '-d', 'cwd', '-Fn'], { encoding: 'utf8' });
    if (output.split('\n').includes('n' + wanted)) return;
  }
  throw new Error(`Preview port ${url.port} belongs to another checkout. Start the chosen source preview and update the binding before refreshing.`);
}
async function capture(root, config, run) {
  checkPreview(root, config);
  const require = createRequire(path.join(root, config.source.appDirectory || '.', 'package.json'));
  const module = await import(pathToFileURL(config.source.playwrightModule || require.resolve('playwright')).href);
  const chromium = module.chromium || module.default?.chromium;
  if (!chromium) throw new Error('The installed Playwright package has no Chromium launcher.');
  const browser = await chromium.launch({ channel: config.source.browserChannel || 'chrome', headless: true });
  const captures = [];
  try {
    for (const screen of config.screens) {
      const context = await browser.newContext({ viewport: screen.viewport || { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
      const page = await context.newPage();
      await page.route('**/*', route => {
        const url = new URL(route.request().url());
        const localFile = url.protocol === 'file:' && fileURLToPath(url).startsWith(path.resolve(root) + path.sep);
        return localFile || ['127.0.0.1', 'localhost'].includes(url.hostname) || ['data:', 'blob:'].includes(url.protocol) ? route.continue() : route.abort();
      });
      const base = config.source.previewFile ? pathToFileURL(path.resolve(root, config.source.previewFile)).href : config.source.previewUrl;
      const url = new URL(screen.url, base);
      if (url.origin !== new URL(base).origin || (url.protocol === 'file:' && !fileURLToPath(url).startsWith(path.resolve(root) + path.sep))) throw new Error('Screen URL escaped the configured local preview.');
      await page.goto(url.href, { waitUntil: 'networkidle', timeout: 45000 });
      await page.locator(screen.selector || 'main').waitFor();
      for (const step of screen.steps || []) {
        const locator = page.getByRole(step.role, { name: step.name, exact: true });
        if (step.optional && !await locator.count()) continue;
        if (step.action === 'range') await locator.evaluate((element, value) => { element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })); }, String(step.value));
        else if (step.action === 'select') await locator.selectOption(step.value);
        else await locator.click();
      }
      await page.mouse.move(0, 0);
      await page.evaluate(() => { window.scrollTo(0, 0); return document.fonts.ready; });
      const fn = await fs.readFile(path.join(here, 'capture.js'), 'utf8');
      const data = await page.evaluate(`(${fn})(${JSON.stringify({ name: screen.name, mainSelector: screen.selector || 'main' })})`);
      await page.screenshot({ path: path.join(run, `${screen.key}-browser.png`), fullPage: true });
      await writeJson(path.join(run, `${screen.key}-capture.json`), data);
      captures.push({ screen, data });
      await context.close();
    }
  } finally { await browser.close(); }
  return captures;
}
async function tokenNodes(nodeId, pageId, out = new Map(), seen = new Set()) {
  if (seen.has(nodeId)) return out;
  seen.add(nodeId);
  const result = await call('get_tree_summary', guardedArgs({ nodeId, pageId, depth: 10 }));
  let last;
  for (const line of result.content.find(x => x.type === 'text').text.split('\n')) {
    const node = line.match(/"([^"]*)" \[([^\]]+)\]/);
    if (node) { last = node[2]; if (node[1].startsWith('__sync_')) out.set(node[1], node[2]); }
    if (line.includes('… ') && last) await tokenNodes(last, pageId, out, seen);
  }
  return out;
}
async function stage(screen, data, source, run, config, captureFingerprint) {
  const info = await api('get_basic_info', { pageId: screen.pageId });
  const bottom = Math.max(0, ...info.artboards.map(b => b.worldY + b.height));
  const board = await api('create_artboard', { pageId: screen.pageId, name: `Refreshing ${screen.name}`,
    styles: { left: screen.left || 0, top: bottom + 3000, width: data.width, height: data.height, backgroundColor: '#FFFFFF' } });
  const nodeId = board.id;
  await writeJson(path.join(run, `${screen.key}-staging.json`), { nodeId, pageId: screen.pageId, source });
  const mapping = {};
  try {
    for (const section of data.sections) {
      await api('write_html', { targetNodeId: nodeId, pageId: screen.pageId, mode: 'insert-children', html: normalizeHtml(section.html) });
    }
    const found = await tokenNodes(nodeId, screen.pageId);
    const names = [];
    for (const [token, id] of found) {
      const descriptor = data.anchors[token];
      if (!descriptor) continue;
      mapping[id] = { ...descriptor, nodeId: id };
      names.push({ nodeId: id, name: descriptor.label });
    }
    for (let i = 0; i < names.length; i += 200) await api('rename_nodes', { updates: names.slice(i, i + 200) });
    if (!Object.keys(mapping).length) throw new Error('No source anchors survived import; retaining the previous screen.');
    const record = { key: screen.key, name: screen.name, nodeId, pageId: screen.pageId, mapping,
      width: data.width, height: data.height, source, captureFingerprint, sourceFiles: screen.sourceFiles || [], run, capturedAt: new Date().toISOString() };
    await writeJson(path.join(run, `${screen.key}-staged.json`), record);
    return record;
  } catch (error) {
    // A partial import is retained for recovery. Never delete content that may have received a pin.
    await api('rename_nodes', { updates: [{ nodeId, name: `Incomplete refresh · ${screen.name}` }] }).catch(() => {});
    throw error;
  }
}
export async function refresh(config, options = {}) {
  projectPath = config.projectPath;
  await checkProject(config);
  const stateFile = path.join(config.stateDirectory, 'state.json');
  const state = await optionalJson(stateFile, { screens: config.initialScreens || {}, history: [] });
  state.history ||= [];
  state.transfers ||= [];
  const source = await sourceSnapshot(sourceRoot(config), config);
  const captureFingerprint = digest(JSON.stringify(config.screens) + await fs.readFile(path.join(here, 'capture.js'), 'utf8') + await fs.readFile(path.join(here, 'html.mjs'), 'utf8'));
  if (!options.force && config.screens.every(s => state.screens[s.key]?.source?.fingerprint === source.fingerprint && state.screens[s.key]?.captureFingerprint === captureFingerprint))
    return { unchanged: true, source, stateFile };
  const run = path.join(config.stateDirectory, 'runs', stamp());
  await fs.mkdir(run, { recursive: true });
  // Original imports predate source keys. Keep those pins on their known original artboards.
  for (const comment of await allComments()) {
    if (mappingFor(comment, state)) continue;
    const node = await api('get_node_info', { nodeId: comment.nodeId, pageId: comment.pageId }).catch(() => null);
    const screen = Object.values(state.screens).find(s => s.nodeId === node?.artboardId);
    if (screen) {
      screen.mapping ||= {};
      screen.mapping[comment.nodeId] = { nodeId: comment.nodeId, key: `legacy:${comment.nodeId}`,
        signature: 'original-import-without-source-identity', label: comment.target.name,
        text: comment.target.text, legacy: true };
    }
  }
  await writeJson(stateFile, state);
  await api('finish_working_on_nodes');
  await fs.cp(config.projectPath, path.join(run, 'project-backup'), { recursive: true });
  const captures = await capture(source.root, config, run);
  await assertSource(source, config);
  const staged = [];
  for (const { screen, data } of captures) staged.push(await stage(screen, data, source, run, config, captureFingerprint));
  await assertSource(source, config);
  const report = { source, run, transferred: [], retained: [], screens: staged.map(s => ({ key: s.key, nodeId: s.nodeId })) };
  for (let i = 0; i < staged.length; i++) {
    await checkProject(config);
    await assertSource(source, config);
    const next = staged[i], screen = config.screens[i], previous = state.screens[screen.key];
    const comments = await allComments();
    for (const comment of comments.filter(c => c.pageId === screen.pageId)) {
      const found = mappingFor(comment, state);
      if (!found || found.screen !== screen.key) continue;
      const match = found.captured.key === '__artboard' ? { nodeId: next.nodeId } : safeMatch(found.captured, Object.values(next.mapping));
      if (match) {
        try {
          const updated = await api('reanchor_comment', { commentId: comment.id, nodeId: match.nodeId, expectedUpdatedAt: comment.updatedAt });
          state.transfers.push({ id: comment.id, beforeUpdatedAt: comment.updatedAt, afterUpdatedAt: updated.updatedAt,
            beforeNodeId: comment.nodeId, afterNodeId: updated.nodeId });
          report.transferred.push(comment.id);
        } catch (error) { report.retained.push({ id: comment.id, reason: error.message }); }
      } else report.retained.push({ id: comment.id, reason: 'No unique unchanged element identity; pin stays on its previous screen.' });
    }
    if (previous) {
      const info = await api('get_basic_info', { pageId: screen.pageId });
      const archiveY = Math.max(next.height, ...info.artboards.filter(b => b.id !== next.nodeId).map(b => b.worldY + b.height)) + 160;
      await api('update_styles', { updates: [{ nodeIds: [previous.nodeId], styles: { top: archiveY } }] });
      await api('rename_nodes', { updates: [{ nodeId: previous.nodeId, name: `Previous · ${screen.name} · ${previous.source?.head?.slice(0, 7) || 'original'}` }] });
      state.history.push(previous);
    }
    await api('update_styles', { updates: [{ nodeIds: [next.nodeId], styles: { left: screen.left || 0, top: 0 } }] });
    await api('rename_nodes', { updates: [{ nodeId: next.nodeId, name: screen.name }] });
    state.screens[screen.key] = next;
    await writeJson(stateFile, state);
  }
  await api('finish_working_on_nodes');
  await writeJson(path.join(run, 'refresh.json'), report);
  return report;
}
async function prepare(config) {
  await refresh(config);
  await checkProject(config);
  const source = await sourceSnapshot(sourceRoot(config), config);
  const state = await readJson(path.join(config.stateDirectory, 'state.json'));
  const comments = (await allComments()).filter(c => c.status === 'open');
  const id = stamp();
  const directory = path.join(config.stateDirectory, 'batches', id);
  const worktree = path.join(config.worktreeDirectory, `design-feedback-${id}`);
  await fs.mkdir(directory, { recursive: true });
  await fs.mkdir(config.worktreeDirectory, { recursive: true });
  git(source.root, 'worktree', 'add', '-b', `codex/design-feedback-${id}`, worktree, source.head);
  await copySource(source, worktree);
  git(worktree, 'add', '--all', '--', ...config.source.inputPaths);
  await assertSource(source, config);
  const batch = { id, projectId: config.projectId, directory, worktree, source, checks: [],
    comments: comments.map(comment => ({ comment, binding: mappingFor(comment, state) })) };
  await writeJson(path.join(directory, 'batch.json'), batch);
  return { batchFile: path.join(directory, 'batch.json'), worktree, comments: batch.comments };
}
async function complete(config, batch, ids) {
  if (!batch.published) throw new Error('Publish verified source changes before completing feedback.');
  await checkProject(config);
  await assertSource(batch.published.source, config);
  const state = await readJson(path.join(config.stateDirectory, 'state.json'));
  if (!config.screens.every(s => state.screens[s.key]?.source?.fingerprint === batch.published.source.fingerprint))
    throw new Error('Refresh the published source into Sketch before resolving comments.');
  const resolved = [], leftOpen = [];
  for (const id of ids) {
    const original = batch.comments.find(c => c.comment.id === id);
    if (!original) throw new Error('Only comments in the initial batch may be completed.');
    const now = await api('get_comment', { commentId: id });
    const mapped = mappingFor(now, state);
    // A refresh legitimately changes the anchor timestamp. The feedback and open status must stay unchanged.
    if (!feedbackStillCurrent(original.comment, now, state.transfers) || now.status !== 'open' || !mapped?.current) {
      leftOpen.push({ id, reason: 'Feedback changed, was closed, or its latest element is uncertain.' }); continue;
    }
    const result = await api('set_comment_status', { commentId: id, status: 'resolved', expectedUpdatedAt: now.updatedAt });
    resolved.push(result.id);
  }
  batch.completed = { resolved, leftOpen, at: new Date().toISOString() };
  await writeJson(path.join(batch.directory, 'batch.json'), batch);
  return batch.completed;
}
async function verifiedMap(config, batch, id, nodeId, evidenceFile) {
  if (!batch.published) throw new Error('Explicit remapping is for intentionally changed elements after publishing and refreshing.');
  await checkProject(config);
  await assertSource(batch.published.source, config);
  const original = batch.comments.find(c => c.comment.id === id);
  if (!original?.binding) throw new Error('This comment has no source-screen binding; inspect the original before mapping it.');
  const stateFile = path.join(config.stateDirectory, 'state.json');
  const state = await readJson(stateFile);
  const screen = state.screens[original.binding.screen];
  if (screen.source.fingerprint !== batch.published.source.fingerprint) throw new Error('Refresh the published source first.');
  if (!screen.mapping[nodeId] && screen.nodeId !== nodeId) throw new Error('The replacement must belong to this screen\'s latest capture.');
  const before = await api('get_comment', { commentId: id });
  if (!feedbackStillCurrent(original.comment, before, state.transfers) || before.status !== 'open')
    throw new Error('The feedback changed during this batch. Leave it open for another review.');
  const evidence = await fs.readFile(evidenceFile);
  if (!evidence.length) throw new Error('Supply the screenshot or browser verification artifact used to confirm the replacement.');
  const target = await api('get_node_info', { nodeId, pageId: before.pageId });
  const after = await api('reanchor_comment', { commentId: id, nodeId, expectedUpdatedAt: before.updatedAt });
  state.transfers.push({ id, beforeUpdatedAt: before.updatedAt, afterUpdatedAt: after.updatedAt,
    beforeNodeId: before.nodeId, afterNodeId: after.nodeId, verifiedForBatch: batch.id,
    evidenceFile: path.resolve(evidenceFile), evidenceSha256: (await import('./core.mjs')).digest(evidence), target });
  await writeJson(stateFile, state);
  return { id, target, evidenceFile };
}

export async function main(argv = process.argv.slice(2)) {
  const args = [...argv];
  let configFile;
  const at = args.indexOf('--config');
  if (at >= 0) { configFile = args[at + 1]; args.splice(at, 2); }
  if (!configFile) {
    const info = await api('get_basic_info');
    configFile = path.join(os.homedir(), 'Documents/Sketch-design/projects', `${info.fileId}.json`);
  }
  const config = await readJson(configFile);
  projectPath = config.projectPath;
  await fs.mkdir(config.stateDirectory, { recursive: true });
  const lock = path.join(config.stateDirectory, 'operation.lock');
  let handle;
  try { handle = await fs.open(lock, 'wx'); await handle.writeFile(JSON.stringify({ pid: process.pid, at: new Date().toISOString() })); }
  catch (error) { if (error.code === 'EEXIST') throw new Error(`Another sync operation holds ${lock}. Check its recorded PID before recovering a stale lock.`); throw error; }
  try {
    const [command = 'status', batchFile, ...extra] = args;
    if (command === 'status') {
      const info = await checkProject(config);
      const source = await sourceSnapshot(sourceRoot(config), config);
      const state = await optionalJson(path.join(config.stateDirectory, 'state.json'), { screens: config.initialScreens || {} });
      return { project: info.fileName, source, screens: config.screens.map(s => ({ key: s.key,
        current: state.screens[s.key]?.source?.fingerprint === source.fingerprint, captured: state.screens[s.key]?.source?.head || 'original' })) };
    }
    if (command === 'refresh') return await refresh(config, { force: args.includes('--force') });
    if (command === 'prepare') return await prepare(config);
    const batch = await readJson(batchFile);
    if (batch.projectId !== config.projectId) throw new Error('Batch belongs to a different project.');
    if (command === 'verify') {
      const check = await verifyCommand(batch, config, extra[0] === '--' ? extra.slice(1) : extra,
        path.join(batch.directory, `check-${batch.checks.length}.log`));
      batch.checks.push(check); await writeJson(batchFile, batch);
      if (check.exitCode) throw new Error(`Verification failed; see ${check.log}`);
      return check;
    }
    if (command === 'publish') { batch.published = await publishFiles(batch, config); await writeJson(batchFile, batch); return batch.published; }
    if (command === 'map') return await verifiedMap(config, batch, ...extra);
    if (command === 'complete') return await complete(config, batch, extra);
    throw new Error('Commands: status, refresh, prepare, verify <batch> -- <command>, publish <batch>, complete <batch> <comment IDs>');
  } finally { await handle.close(); await fs.rm(lock, { force: true }); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then(result => process.stdout.write(JSON.stringify(result, null, 2) + '\n')).catch(error => { console.error(error.message); process.exitCode = 1; });
}
