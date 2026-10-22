// Disposable local fixture. Exercises the real Sketch server without editing AEGIS code.
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { call, payload } from './client.mjs';
import { main } from './sketch-sync.mjs';
import { git, readJson, writeJson } from './core.mjs';

const api = async (name, args = {}) => payload(await call(name, args));
const original = await api('get_basic_info');
const base = await fs.mkdtemp(path.join(os.tmpdir(), 'sketch-sync-e2e-'));
const root = await fs.realpath(base).then(p => path.join(p, 'source'));
await fs.mkdir(path.join(root, 'docs/mockups'), { recursive: true });
const htmlPath = 'docs/mockups/index.html';
await fs.writeFile(path.join(root, htmlPath), '<!doctype html><html><body><main style="width:700px"><h1>A long heading</h1><div class="card"><span>First card</span></div><div class="card"><span>Second card</span></div></main></body></html>');
git(root, 'init', '-q', '-b', 'main'); git(root, 'add', '.');
git(root, '-c', 'user.name=Local test', '-c', 'user.email=local@example.invalid', 'commit', '-qm', 'Sync fixture');
let project;
try {
  project = await api('create_file', { name: 'Sync workflow test' });
  const info = await api('open_file', { fileId: project.id || project.fileId });
  const check = [process.execPath, '-e', "require('node:assert/strict').match(require('node:fs').readFileSync('docs/mockups/index.html','utf8'), /Short heading/)"];
  const config = { projectId: info.fileId, projectPath: info.path, stateDirectory: path.join(base, 'state'), worktreeDirectory: path.join(base, 'worktrees'),
    source: { repository: root, branch: 'main', inputPaths: ['docs/mockups'], previewFile: htmlPath,
      playwrightModule: '/Users/blie/dev/work/firstmate/projects/aegis/prototypes/dot-mscw/node_modules/playwright/index.mjs', requiredChecks: [check] },
    screens: [{ key: 'fixture', name: 'Sync fixture', pageId: info.pages[0].id, url: '#fixture', sourceFiles: [htmlPath] }] };
  const configFile = path.join(base, 'binding.json'); await writeJson(configFile, config);
  const run = (...args) => main([...args, '--config', configFile]);
  await run('refresh');
  let state = await readJson(path.join(config.stateDirectory, 'state.json'));
  let target = Object.values(state.screens.fixture.mapping).find(n => n.text === 'A long heading');
  assert.ok(target, 'captured heading has a source identity');
  const pin = await api('add_comment', { fileId: info.path, nodeId: target.nodeId, body: 'Shorten heading to "Short heading".' });
  const card = Object.values(state.screens.fixture.mapping).find(n => n.tag === 'div' && n.signature.includes('First card'));
  assert.ok(card);
  const cardPin = await api('add_comment', { fileId: info.path, nodeId: card.nodeId, body: 'Keep this pin on the first card' });
  const swapped = (await fs.readFile(path.join(root, htmlPath), 'utf8')).replace('<div class="card"><span>First card</span></div><div class="card"><span>Second card</span></div>', '<div class="card"><span>Second card</span></div><div class="card"><span>First card</span></div>');
  await fs.writeFile(path.join(root, htmlPath), swapped);
  await run('refresh');
  const moved = await api('get_comment', { fileId: info.path, commentId: pin.id });
  assert.equal(moved.id, pin.id); assert.equal(moved.body, pin.body); assert.notEqual(moved.nodeId, pin.nodeId);
  const retained = await api('get_comment', { fileId: info.path, commentId: cardPin.id });
  assert.equal(retained.nodeId, cardPin.nodeId, 'reordered card pin remains on the exact original element');
  const prepared = await run('prepare');
  const batchFile = prepared.batchFile;
  const file = path.join(prepared.worktree, htmlPath);
  await fs.writeFile(file, (await fs.readFile(file, 'utf8')).replace('A long heading', 'Short heading'));
  await run('verify', batchFile, '--', ...check);
  await run('publish', batchFile);
  assert.match(await fs.readFile(path.join(root, htmlPath), 'utf8'), /Short heading/);
  const refreshed = await run('refresh');
  state = await readJson(path.join(config.stateDirectory, 'state.json'));
  target = Object.values(state.screens.fixture.mapping).find(n => n.text === 'Short heading');
  assert.ok(target);
  await run('map', batchFile, pin.id, target.nodeId, path.join(refreshed.run, 'fixture-browser.png'));
  await run('complete', batchFile, pin.id);
  const done = await api('get_comment', { fileId: info.path, commentId: pin.id });
  assert.equal((await api('get_comment', {fileId: info.path, commentId: cardPin.id})).status, 'open');
  assert.equal(done.id, pin.id); assert.equal(done.body, pin.body); assert.equal(done.status, 'resolved'); assert.equal(done.nodeId, target.nodeId);
  console.log(JSON.stringify({ passed: true, base, projectPath: info.path, pinId: pin.id,
    stages: ['capture', 'pin', 'refresh and transfer', 'isolate source', 'verify', 'publish', 'refresh changed heading', 'verify replacement', 'resolve'] }));
} finally {
  await api('open_file', { fileId: original.path });
}
