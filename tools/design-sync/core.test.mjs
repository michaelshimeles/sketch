import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { git, sourceSnapshot, sourceRoot, assertSource, safeMatch, feedbackStillCurrent,
  copySource, publishFiles, verifyCommand } from './core.mjs';

async function fixture(t) {
  const base = await fs.mkdtemp(path.join(os.tmpdir(), 'sketch-sync-test-'));
  const root = await fs.realpath(base).then(p => path.join(p, 'project'));
  await fs.mkdir(path.join(root, 'src'), { recursive: true });
  await fs.writeFile(path.join(root, 'src/title.txt'), 'Original\n');
  git(root, 'init', '-q', '-b', 'main'); git(root, 'add', '.');
  git(root, '-c', 'user.name=Local test', '-c', 'user.email=local@example.invalid', 'commit', '-qm', 'Initial fixture');
  const config = { source: { repository: root, branch: 'main', inputPaths: ['src'] } };
  const source = await sourceSnapshot(root, config);
  const worktree = path.join(base, 'feedback');
  git(root, 'worktree', 'add', '--detach', worktree, source.head);
  await copySource(source, worktree);
  const batch = { directory: base, source, worktree, checks: [] };
  t.after(async () => { await fs.rm(base, { recursive: true, force: true }); });
  return { base, root, config, batch };
}
test('only one matching source identity may receive a pin', () => {
  const old = { key: 'card > h2', signature: 'Heading' };
  const same = { ...old, nodeId: 'new:2' };
  assert.equal(safeMatch(old, [same]).nodeId, 'new:2');
  assert.equal(safeMatch(old, [same, same]), null);
  assert.equal(safeMatch(old, [same, {...same,key:'different position'}]), null);
  assert.equal(safeMatch({...old,ambiguous:true}, [same]), null);
  assert.equal(safeMatch(old, [{ ...same, signature: 'Different card' }]), null);
});
test('only recorded anchor transfers can change the feedback timestamp', () => {
  const before = { id: 'pin', body: 'Shorten heading', status: 'open', updatedAt: 10, nodeId: 'old:1' };
  const after = { ...before, updatedAt: 12, nodeId: 'new:2' };
  const transfers = [{ id: 'pin', beforeUpdatedAt: 10, afterUpdatedAt: 12, beforeNodeId: 'old:1', afterNodeId: 'new:2' }];
  assert.equal(feedbackStillCurrent(before, after, transfers), true);
  assert.equal(feedbackStillCurrent(before, { ...after, body: 'Edited' }, transfers), false);
  assert.equal(feedbackStillCurrent(before, { ...after, updatedAt: 13 }, transfers), false);
});
test('source fingerprints include unsaved-to-Git edits and untracked source', async t => {
  const { root, config, batch } = await fixture(t);
  assert.equal(sourceRoot(config), root);
  await fs.writeFile(path.join(root, 'src/new.txt'), 'New file');
  await assert.rejects(assertSource(batch.source, config), /source changed/);
});
test('verified changes publish back to the unchanged source', async t => {
  const { base, root, config, batch } = await fixture(t);
  await fs.writeFile(path.join(batch.worktree, 'src/title.txt'), 'Shorter\n');
  batch.checks.push(await verifyCommand(batch, config, [process.execPath, '-e', 'process.exit(0)'], path.join(base, 'check.log')));
  const result = await publishFiles(batch, config);
  assert.deepEqual(result.files, ['src/title.txt']);
  assert.equal(await fs.readFile(path.join(root, 'src/title.txt'), 'utf8'), 'Shorter\n');
});
test('concurrent Firstmate edits are not overwritten', async t => {
  const { base, root, config, batch } = await fixture(t);
  await fs.writeFile(path.join(batch.worktree, 'src/title.txt'), 'Requested change\n');
  batch.checks.push(await verifyCommand(batch, config, [process.execPath, '-e', 'process.exit(0)'], path.join(base, 'check.log')));
  await fs.writeFile(path.join(root, 'src/title.txt'), 'Firstmate newer work\n');
  await assert.rejects(publishFiles(batch, config), /source changed/);
  assert.equal(await fs.readFile(path.join(root, 'src/title.txt'), 'utf8'), 'Firstmate newer work\n');
});
test('edits after verification and failed checks cannot publish', async t => {
  const { base, config, batch } = await fixture(t);
  await fs.writeFile(path.join(batch.worktree, 'src/title.txt'), 'Change\n');
  batch.checks.push(await verifyCommand(batch, config, [process.execPath, '-e', 'process.exit(1)'], path.join(base, 'fail.log')));
  await assert.rejects(publishFiles(batch, config), /verify successfully/);
  batch.checks.push(await verifyCommand(batch, config, [process.execPath, '-e', 'process.exit(0)'], path.join(base, 'pass.log')));
  await fs.writeFile(path.join(batch.worktree, 'src/title.txt'), 'Changed again\n');
  await assert.rejects(publishFiles(batch, config), /verify successfully/);
});

test('atomic exchange detects an edit that races the preflight and retains its bytes', async t => {
  const { base, root } = await fixture(t);
  const { execFileSync } = await import('node:child_process');
  const { digest } = await import('./core.mjs');
  const file = path.join(root, 'src/title.txt');
  const expected = digest('Original\n');
  const proposal = path.join(base, 'proposal');
  await fs.writeFile(proposal, 'Requested\n');
  await fs.writeFile(file, 'Concurrent writer\n');
  assert.throws(() => execFileSync('python3', [new URL('./exchange.py', import.meta.url).pathname, file, expected, proposal, path.join(base,'recovery')], {stdio:'pipe'}), /failed/);
  assert.equal(await fs.readFile(file, 'utf8'), 'Concurrent writer\n');
  const artifacts = await fs.readdir(path.join(base, 'recovery'));
  assert.ok(artifacts.length >= 2);
});

test('staged out-of-scope files cannot affect verification and disappear on publish', async t => {
  const {base,config,batch}=await fixture(t);
  await fs.writeFile(path.join(batch.worktree,'src/title.txt'),'Changed\n');
  await fs.writeFile(path.join(batch.worktree,'unbound-test-config.txt'),'Altered test setup');
  git(batch.worktree,'add','unbound-test-config.txt');
  batch.checks.push(await verifyCommand(batch,config,[process.execPath,'-e','process.exit(0)'],path.join(base,'check.log')));
  await assert.rejects(publishFiles(batch,config),/outside its source binding/);
});
test('explicit current node identity is valid even when automatic remapping is ambiguous', async () => {
  const {mappingFor}=await import('./sketch-sync.mjs');
  const anchor={key:'same-label',signature:'same',ambiguous:true,nodeId:'page:1'};
  const state={screens:{screen:{key:'screen',mapping:{'page:1':anchor},source:{}}},history:[]};
  assert.equal(mappingFor({nodeId:'page:1'},state).current.nodeId,'page:1');
  state.history.push({key:'screen',mapping:{'old:1':{...anchor,nodeId:'old:1'}},source:{}});
  assert.equal(mappingFor({nodeId:'old:1'},state).current,null);
});
