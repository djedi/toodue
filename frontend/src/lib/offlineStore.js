const SNAPSHOT_KEY = 'toodue.offline.snapshot.v1';
const QUEUE_KEY = 'toodue.offline.queue.v1';
const TEMP_ID_KEY = 'toodue.offline.nextTempId.v1';

function storageOrDefault(storage) {
  return storage ?? globalThis.localStorage;
}

function safeJsonParse(raw, fallback) {
  if (!raw) return fallback;
  try {
    return JSON.parse(raw);
  } catch {
    return fallback;
  }
}

export function createMemoryStorage() {
  const values = new Map();
  return {
    getItem: (key) => (values.has(key) ? values.get(key) : null),
    setItem: (key, value) => values.set(key, String(value)),
    removeItem: (key) => values.delete(key),
    clear: () => values.clear()
  };
}

export function loadSnapshot(storage) {
  return safeJsonParse(storageOrDefault(storage).getItem(SNAPSHOT_KEY), null);
}

export function saveSnapshot({ user, projects, tasks }, storage) {
  storageOrDefault(storage).setItem(SNAPSHOT_KEY, JSON.stringify({ user, projects, tasks }));
}

export function clearOfflineState(storage) {
  const s = storageOrDefault(storage);
  s.removeItem(SNAPSHOT_KEY);
  s.removeItem(QUEUE_KEY);
  s.removeItem(TEMP_ID_KEY);
}

export function loadQueue(storage) {
  return safeJsonParse(storageOrDefault(storage).getItem(QUEUE_KEY), []);
}

export function saveQueue(queue, storage) {
  storageOrDefault(storage).setItem(QUEUE_KEY, JSON.stringify(queue));
}

export function hasQueuedOperations(storage) {
  return loadQueue(storage).length > 0;
}

export function nextTempId(storage) {
  const s = storageOrDefault(storage);
  const current = Number(s.getItem(TEMP_ID_KEY) || '-1');
  s.setItem(TEMP_ID_KEY, String(current - 1));
  return current;
}

export function createOfflineTask(fields, { userId, now = new Date().toISOString(), tempId } = {}) {
  return {
    id: tempId ?? nextTempId(),
    project_id: fields.project_id,
    parent_id: fields.parent_id ?? null,
    creator_id: userId,
    name: fields.name ?? '',
    description: fields.description ?? '',
    due_date: fields.due_date ?? null,
    due_time: fields.due_time ?? null,
    deadline: fields.deadline ?? null,
    priority: fields.priority ?? 4,
    completed_at: null,
    sort_order: 0,
    created_at: now,
    updated_at: now,
    comment_count: 0,
    attachment_count: 0,
    subtask_count: 0,
    subtask_done_count: 0,
    _offline: true,
    _offlineBody: { ...fields }
  };
}

function taskCreateBody(task) {
  if (task._offlineBody) return { ...task._offlineBody };

  const body = {
    project_id: task.project_id,
    name: task.name
  };
  for (const key of [
    'parent_id',
    'description',
    'due_date',
    'due_time',
    'deadline',
    'priority',
    'sort_order'
  ]) {
    if (task[key] !== undefined && task[key] !== null && task[key] !== '') body[key] = task[key];
  }
  return body;
}

export function queueTaskCreate(task, storage) {
  const queue = loadQueue(storage);
  queue.push({ type: 'task.create', tempId: task.id, body: taskCreateBody(task) });
  saveQueue(queue, storage);
}

export function queueTaskPatch(id, body, storage) {
  const queue = loadQueue(storage);
  const pendingCreate = queue.find((op) => op.type === 'task.create' && op.tempId === id);
  if (pendingCreate) {
    if (body.completed === true) {
      saveQueue(queue.filter((op) => op !== pendingCreate), storage);
      return;
    }
    const { completed, ...createFields } = body;
    pendingCreate.body = { ...pendingCreate.body, ...createFields };
  } else {
    queue.push({ type: 'task.patch', id, body });
  }
  saveQueue(queue, storage);
}

export function queueTaskDelete(id, storage) {
  const queue = loadQueue(storage);
  const createIndex = queue.findIndex((op) => op.type === 'task.create' && op.tempId === id);
  if (createIndex >= 0) {
    queue.splice(createIndex, 1);
  } else {
    queue.push({ type: 'task.delete', id });
  }
  saveQueue(queue, storage);
}

export function queueProjectCreate(project, storage) {
  const queue = loadQueue(storage);
  queue.push({ type: 'project.create', tempId: project.id, body: { name: project.name } });
  saveQueue(queue, storage);
}

export function queueProjectPatch(id, body, storage) {
  const queue = loadQueue(storage);
  const pendingCreate = queue.find((op) => op.type === 'project.create' && op.tempId === id);
  if (pendingCreate) pendingCreate.body = { ...pendingCreate.body, ...body };
  else queue.push({ type: 'project.patch', id, body });
  saveQueue(queue, storage);
}

export function queueProjectDelete(id, storage) {
  const queue = loadQueue(storage);
  const createIndex = queue.findIndex((op) => op.type === 'project.create' && op.tempId === id);
  if (createIndex >= 0) queue.splice(createIndex, 1);
  else queue.push({ type: 'project.delete', id });
  saveQueue(queue, storage);
}

export function createOfflineProject(fields, { userId, now = new Date().toISOString(), tempId } = {}) {
  return {
    id: tempId ?? nextTempId(),
    name: fields.name ?? '',
    color: fields.color ?? '#dd4b33',
    parent_id: fields.parent_id ?? null,
    owner_id: userId,
    is_inbox: 0,
    sort_order: 0,
    created_at: now,
    active_count: 0,
    members: []
  };
}

async function replayOperation(op, client, idMap) {
  const mapId = (id) => idMap.get(id) ?? id;
  switch (op.type) {
    case 'task.create': {
      const body = { ...op.body };
      if (body.project_id !== undefined) body.project_id = mapId(body.project_id);
      if (body.parent_id !== undefined) body.parent_id = mapId(body.parent_id);
      const created = await client.request('POST', '/tasks', body);
      if (op.tempId !== undefined) idMap.set(op.tempId, created?.id);
      return;
    }
    case 'task.patch':
      await client.request('PATCH', `/tasks/${mapId(op.id)}`, op.body);
      return;
    case 'task.delete':
      await client.request('DELETE', `/tasks/${mapId(op.id)}`);
      return;
    case 'project.create': {
      const created = await client.request('POST', '/projects', op.body);
      if (op.tempId !== undefined) idMap.set(op.tempId, created?.id);
      return;
    }
    case 'project.patch':
      await client.request('PATCH', `/projects/${mapId(op.id)}`, {
        ...op.body,
        parent_id: op.body?.parent_id == null ? op.body?.parent_id : mapId(op.body.parent_id)
      });
      return;
    case 'project.delete':
      await client.request('DELETE', `/projects/${mapId(op.id)}`);
      return;
    default:
      throw new Error(`Unknown offline operation: ${op.type}`);
  }
}

export async function replayQueuedOperations(client, storage) {
  const queue = loadQueue(storage);
  if (!queue.length) return { replayed: 0 };

  const idMap = new Map();
  let replayed = 0;
  for (const op of queue) {
    await replayOperation(op, client, idMap);
    replayed += 1;
    saveQueue(queue.slice(replayed), storage);
  }
  saveQueue([], storage);
  await client.refresh?.();
  return { replayed };
}
