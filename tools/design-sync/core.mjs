import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawn } from 'node:child_process';

export const digest = value => crypto.createHash('sha256').update(value).digest('hex');
export const readJson = async file => JSON.parse(await fs.readFile(file, 'utf8'));
export async function writeJson(file, value) {
  await fs.mkdir(path.dirname(file), { recursive: true });
  const temp = `${file}.${process.pid}.tmp`;
  await fs.writeFile(temp, JSON.stringify(value, null, 2) + '\n');
  await fs.rename(temp, file);
}
export function git(root, ...args) {
  return execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
}
export function sourceRoot(config) {
  const rows = git(config.source.repository, 'worktree', 'list', '--porcelain').trim().split('\n\n');
  const trees = rows.map(row => Object.fromEntries(row.split('\n').map(line => {
    const at = line.indexOf(' '); return [line.slice(0, at), line.slice(at + 1)];
  })));
  const worker = trees.find(t => t.branch === `refs/heads/${config.source.branch}`);
  const main = trees.find(t => t.branch === 'refs/heads/main');
  if (worker) {
    // A finished worker branch should not mask newer changes on main.
    if (main && !git(worker.worktree, 'status', '--porcelain').trim()) {
      try { git(main.worktree, 'merge-base', '--is-ancestor', worker.HEAD, main.HEAD); return main.worktree; } catch {}
    }
    return worker.worktree;
  }
  if (main) return main.worktree;
  throw new Error('The configured worker and main checkouts are unavailable. Rebind the source.');
}
export async function sourceSnapshot(root, config) {
  root = await fs.realpath(root);
  const headNames = git(root, 'ls-tree', '-r', '--name-only', '-z', 'HEAD', '--', ...config.source.inputPaths).split('\0').filter(Boolean);
  const files = git(root, 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', ...config.source.inputPaths)
    .split('\0').filter(Boolean).concat(headNames).sort();
  const entries = {};
  for (const name of new Set(files)) {
    const file = path.resolve(root, name);
    if (!file.startsWith(path.resolve(root) + path.sep)) throw new Error(`Invalid source path ${name}`);
    try {
      const stat = await fs.lstat(file);
      if (stat.isSymbolicLink()) throw new Error(`Source symlinks need explicit review: ${name}`);
      entries[name] = { hash: digest(await fs.readFile(file)), mode: stat.mode & 0o777 };
    } catch (error) { if (error.code !== 'ENOENT') throw error; entries[name] = null; }
  }
  const head = git(root, 'rev-parse', 'HEAD').trim();
  return { root, head, branch: git(root, 'branch', '--show-current').trim(), entries,
    fingerprint: digest(JSON.stringify({ head, entries })) };
}
export async function assertSource(expected, config) {
  const root = sourceRoot(config);
  const current = await sourceSnapshot(root, config);
  if (await fs.realpath(root) !== await fs.realpath(expected.root) || current.fingerprint !== expected.fingerprint)
    throw new Error('Firstmate source changed during this operation. Refresh and prepare a new batch; no source files were overwritten.');
  return current;
}
export function safeMatch(previous, candidates) {
  if (!previous || previous.ambiguous) return null;
  if (candidates.filter(next => next.signature === previous.signature).length !== 1) return null;
  const found = candidates.filter(next => next.key === previous.key && next.signature === previous.signature);
  return found.length === 1 ? found[0] : null;
}
export function commentUnchanged(before, after) {
  return before.id === after.id && before.body === after.body && before.status === after.status
    && before.updatedAt === after.updatedAt && before.nodeId === after.nodeId;
}
export function feedbackStillCurrent(before, after, transfers = []) {
  if (before.id !== after.id || before.body !== after.body || before.status !== after.status) return false;
  let timestamp = before.updatedAt, node = before.nodeId;
  for (const transfer of transfers) {
    if (transfer.id === before.id && transfer.beforeUpdatedAt === timestamp && transfer.beforeNodeId === node) {
      timestamp = transfer.afterUpdatedAt; node = transfer.afterNodeId;
    }
  }
  return timestamp === after.updatedAt && node === after.nodeId;
}
export async function copySource(snapshot, destination) {
  for (const [name, entry] of Object.entries(snapshot.entries)) {
    const output = path.join(destination, name);
    if (entry === null) { await fs.rm(output, { force: true }); continue; }
    const bytes = await fs.readFile(path.join(snapshot.root, name));
    if (digest(bytes) !== entry.hash) throw new Error('Source changed while copying; prepare again.');
    await fs.mkdir(path.dirname(output), { recursive: true });
    await fs.writeFile(output, bytes, { mode: entry.mode });
  }
}
export async function changesBetween(base, work) {
  const names = new Set([...Object.keys(base.entries), ...Object.keys(work.entries)]);
  return [...names].filter(name => JSON.stringify(base.entries[name]) !== JSON.stringify(work.entries[name]));
}
export async function publishFiles(batch, config) {
  if (batch.published) throw new Error('This batch was already published.');
  await assertSource(batch.source, config);
  const current = await sourceSnapshot(batch.worktree, config);
  if (current.head !== batch.source.head) throw new Error('Keep batch source edits uncommitted until publishing; its baseline HEAD changed.');
  const changed = [...git(batch.worktree, 'diff', 'HEAD', '--name-only', '-z').split('\0'), ...git(batch.worktree, 'ls-files', '--others', '--exclude-standard', '-z').split('\0')].filter(Boolean);
  const within = (name, paths) => paths.some(p => name === p || name.startsWith(p.replace(/\/$/, '') + '/'));
  const outside = changed.filter(name => !within(name, config.source.inputPaths) && !within(name, config.source.generatedPaths || []));
  if (outside.length) throw new Error(`Batch edits outside its source binding: ${outside.join(', ')}`);
  const checks = batch.checks || [];
  if (!checks.some(c => c.exitCode === 0 && c.fingerprint === current.fingerprint))
    throw new Error('Run verify successfully against the current worktree before publishing.');
  for (const required of config.source.requiredChecks || []) {
    if (!checks.some(c => c.exitCode === 0 && c.fingerprint === current.fingerprint && JSON.stringify(c.command) === JSON.stringify(required)))
      throw new Error(`Required check missing: ${required.join(' ')}`);
  }
  const names = await changesBetween(batch.source, current);
  if (!names.length) throw new Error('No source changes in this batch.');
  const recovery = path.join(batch.directory, 'source-preimages');
  await fs.mkdir(recovery, { recursive: true });
  const written = [];
  const apply = (file, expected, replacement) => JSON.parse(execFileSync('python3', [
    fileURLToPath(new URL('./exchange.py', import.meta.url)), file, expected || 'absent', replacement || 'absent', recovery,
  ], { encoding: 'utf8' }));
  try {
    await assertSource(batch.source, config);
    for (const [index, name] of names.entries()) {
      const file = path.join(batch.source.root, name);
      const entry = current.entries[name];
      let proposal = null;
      if (entry) {
        const bytes = await fs.readFile(path.join(batch.worktree, name));
        if (digest(bytes) !== entry.hash) throw new Error('The prepared worktree changed during publish.');
        proposal = path.join(recovery, `proposal-${index}`);
        await fs.writeFile(proposal, bytes, { mode: entry.mode });
      }
      await fs.mkdir(path.dirname(file), { recursive: true });
      const result = apply(file, batch.source.entries[name]?.hash, proposal);
      written.push({ file, hash: entry?.hash, preimage: result.preimage });
    }
    const result = await sourceSnapshot(batch.source.root, config);
    if (result.fingerprint !== current.fingerprint) throw new Error('Source changed while publishing.');
    return { source: result, files: names, recovery };
  } catch (error) {
    for (const item of written.reverse()) {
      try { apply(item.file, item.hash, item.preimage); } catch { /* Exact displaced versions remain in recovery. */ }
    }
    throw new Error(`${error.message} Source preimages and conflict versions are retained at ${recovery}. Leave feedback open.`);
  }
}

export async function verifyCommand(batch, config, command, log) {
  if (!command.length) throw new Error('verify needs a command after --');
  const before = await sourceSnapshot(batch.worktree, config);
  const output = await fs.open(log, 'w');
  let exitCode;
  try {
    exitCode = await new Promise((resolve, reject) => {
      const child = spawn(command[0], command.slice(1), { cwd: batch.worktree, stdio: ['ignore', output.fd, output.fd] });
      child.once('error', reject); child.once('exit', code => resolve(code ?? 1));
    });
  } finally { await output.close(); }
  const after = await sourceSnapshot(batch.worktree, config);
  if (before.fingerprint !== after.fingerprint) throw new Error('Verification changed source files. Check the diff and rerun verification.');
  return { command, exitCode, fingerprint: after.fingerprint, log, at: new Date().toISOString() };
}
