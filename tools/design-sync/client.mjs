import fs from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

const endpoint = process.env.SKETCH_MCP_URL || 'http://127.0.0.1:29980/mcp';
let session;
let nextId = 1;
async function request(method, params, notification = false) {
  const body = { jsonrpc: '2.0', method, params };
  if (!notification) body.id = nextId++;
  const headers = { 'Content-Type': 'application/json', Accept: 'application/json, text/event-stream' };
  if (session) headers['Mcp-Session-Id'] = session;
  const response = await fetch(endpoint, { method: 'POST', headers, body: JSON.stringify(body), signal: AbortSignal.timeout(120000) });
  session = response.headers.get('mcp-session-id') || session;
  if (response.status === 404 && session) { session = null; throw new Error('Sketch restarted. Run the command again to reconnect.'); }
  if (!response.ok) throw new Error(`${response.status}: ${await response.text()}`);
  let value = {};
  if (response.status === 202 || response.status === 204) return value;
  if (response.headers.get('content-type')?.includes('application/json')) value = await response.json();
  else {
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      buffer += decoder.decode(chunk.value, {stream:true});
      const lines = buffer.split('\n');
      buffer = lines.pop();
      let found = false;
      for (const line of lines) {
        if (line.startsWith('data: {')) {
          const parsed = JSON.parse(line.slice(6));
          if (parsed.id === body.id) { value = parsed; found = true; break; }
        }
      }
      if (found) { await reader.cancel(); break; }
    }
  }
  if (value.error) throw new Error(JSON.stringify(value.error));
  return value.result;
}
export async function call(name, args = {}) {
  if (!session) {
    await request('initialize', { protocolVersion: '2025-03-26', capabilities: {}, clientInfo: { name: 'sketch-design-client', version: '1.0' } });
    await request('notifications/initialized', {}, true);
  }
  const result = await request('tools/call', { name, arguments: args });
  if (result.isError) throw new Error(JSON.stringify(result));
  return result;
}
export function payload(result) {
  for (const item of result.content || []) {
    if (item.type === 'text') {
      try { return JSON.parse(item.text); } catch {}
    }
  }
  return result;
}
export async function screenshot(nodeId, path) {
  const result = await call('get_screenshot', { nodeId, scale: 1 });
  const item = result.content.find(item => item.type === 'image');
  if (!item) throw new Error(JSON.stringify(result));
  await fs.writeFile(path, Buffer.from(item.data, 'base64'));
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const result = await call(process.argv[2], JSON.parse(process.argv[3] || '{}'));
  console.log(JSON.stringify(result));
}
