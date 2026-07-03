#!/usr/bin/env node
const API_URL = (process.env.TOODUE_API_URL || 'https://app.toodue.com').replace(/\/$/, '');
const API_KEY = process.env.TOODUE_API_KEY;

function reply(id, result) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
}
function error(id, code, message) {
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } }) + '\n');
}
async function api(path, { method = 'GET', body } = {}) {
  if (!API_KEY) throw new Error('TOODUE_API_KEY is required');
  const res = await fetch(`${API_URL}/api/ai${path}`, {
    method,
    headers: {
      authorization: `Bearer ${API_KEY}`,
      ...(body ? { 'content-type': 'application/json' } : {})
    },
    body: body ? JSON.stringify(body) : undefined
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) throw new Error(data?.error || `${res.status} ${res.statusText}`);
  return data;
}
const toolDefs = [
  { name: 'toodue_me', description: 'Return the TooDue account associated with the API key.', inputSchema: { type: 'object', properties: {} } },
  { name: 'toodue_list_projects', description: 'List projects visible to the account.', inputSchema: { type: 'object', properties: {} } },
  { name: 'toodue_create_project', description: 'Create a TooDue project.', inputSchema: { type: 'object', required: ['name'], properties: { name: { type: 'string' }, parent_id: { type: 'number' }, color: { type: 'string' } } } },
  { name: 'toodue_list_tasks', description: 'List incomplete tasks, optionally by project.', inputSchema: { type: 'object', properties: { project_id: { type: 'number' }, completed: { type: 'boolean' } } } },
  { name: 'toodue_search_tasks', description: 'Search incomplete tasks by name or description.', inputSchema: { type: 'object', required: ['q'], properties: { q: { type: 'string' }, limit: { type: 'number' } } } },
  { name: 'toodue_create_task', description: 'Create a task.', inputSchema: { type: 'object', required: ['project_id', 'name'], properties: { project_id: { type: 'number' }, parent_id: { type: 'number' }, name: { type: 'string' }, description: { type: 'string' }, due_date: { type: 'string' }, due_time: { type: 'string' }, deadline: { type: 'string' }, priority: { type: 'number' } } } },
  { name: 'toodue_update_task', description: 'Update a task.', inputSchema: { type: 'object', required: ['id'], properties: { id: { type: 'number' }, name: { type: 'string' }, description: { type: 'string' }, due_date: { type: ['string', 'null'] }, due_time: { type: ['string', 'null'] }, deadline: { type: ['string', 'null'] }, priority: { type: 'number' }, project_id: { type: 'number' }, completed: { type: 'boolean' } } } },
  { name: 'toodue_complete_task', description: 'Mark a task complete or incomplete.', inputSchema: { type: 'object', required: ['id'], properties: { id: { type: 'number' }, completed: { type: 'boolean', default: true } } } },
  { name: 'toodue_delete_task', description: 'Delete a task.', inputSchema: { type: 'object', required: ['id'], properties: { id: { type: 'number' } } } }
];
async function callTool(name, args = {}) {
  switch (name) {
    case 'toodue_me': return api('/me');
    case 'toodue_list_projects': return api('/projects');
    case 'toodue_create_project': return api('/projects', { method: 'POST', body: args });
    case 'toodue_list_tasks': {
      const qs = new URLSearchParams();
      if (args.project_id) qs.set('project_id', args.project_id);
      if (args.completed !== undefined) qs.set('completed', String(args.completed));
      return api(`/tasks${qs.size ? `?${qs}` : ''}`);
    }
    case 'toodue_search_tasks': {
      const qs = new URLSearchParams({ q: args.q, limit: String(args.limit || 25) });
      return api(`/tasks/search?${qs}`);
    }
    case 'toodue_create_task': return api('/tasks', { method: 'POST', body: args });
    case 'toodue_update_task': {
      const { id, ...body } = args;
      return api(`/tasks/${id}`, { method: 'PATCH', body });
    }
    case 'toodue_complete_task': return api(`/tasks/${args.id}`, { method: 'PATCH', body: { completed: args.completed ?? true } });
    case 'toodue_delete_task': return api(`/tasks/${args.id}`, { method: 'DELETE' });
    default: throw new Error(`Unknown tool: ${name}`);
  }
}
let buffer = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
  buffer += chunk;
  let idx;
  while ((idx = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, idx).trim();
    buffer = buffer.slice(idx + 1);
    if (!line) continue;
    handle(JSON.parse(line)).catch((e) => error(null, -32603, e.message));
  }
});
async function handle(msg) {
  const { id, method, params = {} } = msg;
  try {
    if (method === 'initialize') {
      reply(id, { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'toodue-mcp', version: '0.1.0' } });
    } else if (method === 'notifications/initialized') {
      return;
    } else if (method === 'tools/list') {
      reply(id, { tools: toolDefs });
    } else if (method === 'tools/call') {
      const data = await callTool(params.name, params.arguments || {});
      reply(id, { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }] });
    } else {
      error(id, -32601, `Method not found: ${method}`);
    }
  } catch (e) {
    error(id, -32000, e.message);
  }
}
