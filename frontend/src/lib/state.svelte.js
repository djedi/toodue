import { api, request, setUnauthorizedHandler, isOfflineError } from './api.js';
import { todayStr } from './dates.js';
import { apiUrl, setSessionToken } from './native.js';
import {
  clearOfflineState,
  createOfflineProject,
  createOfflineTask,
  hasQueuedOperations,
  loadSnapshot,
  queueProjectCreate,
  queueProjectDelete,
  queueProjectPatch,
  queueTaskCreate,
  queueTaskDelete,
  queueTaskPatch,
  replayQueuedOperations,
  saveSnapshot
} from './offlineStore.js';

export const data = $state({
  user: undefined, // undefined = booting, null = signed out
  projects: [],
  tasks: [],
  templates: [],
  ready: false,
  offline: typeof navigator !== 'undefined' ? !navigator.onLine : false,
  syncPending: false
});

export const ui = $state({
  view: 'today',
  projectId: null,
  openTaskId: null,
  swipedTaskId: null, // task row whose swipe-to-delete action is revealed (mobile)
  quickAdd: null, // null or { project_id?, due_date?, parent_id? }
  showSettings: false,
  toast: null,
  theme: 'system',
  colorScheme: 'sky'
});

// Components (e.g. the task modal) listen here for raw server events.
export const bus = new EventTarget();

setUnauthorizedHandler(() => {
  resetMutationState();
  if (!data.offline && data.user) data.user = null;
});

/* ---------- theme ---------- */

function systemPrefersDark() {
  // Prefer an explicit light OS signal when available. Some embedded browsers can report
  // dark aggressively; this keeps System aligned with macOS/iOS light mode when it says so.
  if (matchMedia('(prefers-color-scheme: light)').matches) return false;
  return matchMedia('(prefers-color-scheme: dark)').matches;
}

export function applyTheme() {
  const dark = ui.theme === 'dark' || (ui.theme === 'system' && systemPrefersDark());
  document.documentElement.classList.toggle('dark', dark);
  document.documentElement.dataset.colorScheme = ui.colorScheme || 'sky';
}

export function setTheme(t) {
  ui.theme = t;
  localStorage.setItem('toodue-theme', t);
  applyTheme();
}

export function setColorScheme(s) {
  ui.colorScheme = s;
  localStorage.setItem('toodue-color-scheme', s);
  applyTheme();
}

/* ---------- toast ---------- */

let toastTimer;
export function toast(msg) {
  ui.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast = null), 3500);
}

function saveCurrentSnapshot() {
  if (data.user) saveSnapshot({ user: data.user, projects: data.projects, tasks: data.tasks });
}

function restoreSnapshot() {
  const snapshot = loadSnapshot();
  if (!snapshot?.user) return false;
  data.user = snapshot.user;
  data.projects = snapshot.projects ?? [];
  data.tasks = snapshot.tasks ?? [];
  data.ready = true;
  data.offline = true;
  return true;
}

function markOffline() {
  data.offline = true;
}

function markOnline() {
  data.offline = false;
  syncOfflineQueue().catch(() => {});
}

function mutationOffline(err) {
  if (!isOfflineError(err)) return false;
  markOffline();
  return true;
}

function applyLocalTaskPatch(task, fields) {
  const now = new Date().toISOString();
  const updated = { ...task, updated_at: now, _offline: true };
  if ('completed' in fields) updated.completed_at = fields.completed ? now : null;
  for (const key of ['name', 'description', 'due_date', 'due_time', 'deadline', 'repeat_rule', 'priority', 'project_id']) {
    if (key in fields) updated[key] = fields[key];
  }
  if (fields.due_date === null) {
    updated.due_time = null;
    updated.repeat_rule = null;
  }
  return updated;
}

function offlineToast() {
  toast('Saved offline — will sync when you reconnect.');
}

function requireOnlineAction() {
  if (data.offline || (typeof navigator !== 'undefined' && navigator.onLine === false)) {
    markOffline();
    toast('That action needs a connection. Core task/project edits still work offline.');
    return false;
  }
  return true;
}

/* ---------- routing (hash based, PWA friendly) ---------- */

export function navigate(view, projectId = null) {
  location.hash = view === 'project' ? `#/project/${projectId}` : `#/${view}`;
}

export function syncRoute() {
  ui.swipedTaskId = null;
  const parts = location.hash.replace(/^#\/?/, '').split('/');
  if (parts[0] === 'project' && parts[1]) {
    ui.view = 'project';
    ui.projectId = Number(parts[1]);
  } else if (['inbox', 'today', 'upcoming', 'projects'].includes(parts[0])) {
    ui.view = parts[0];
    ui.projectId = null;
  } else if (parts[0] === 'browse') {
    // Old name for the Projects tab; keep bookmarks working.
    ui.view = 'projects';
    ui.projectId = null;
  } else {
    ui.view = 'today';
    ui.projectId = null;
  }
}

/* ---------- data loading ---------- */

export async function boot() {
  ui.theme = localStorage.getItem('toodue-theme') || 'system';
  ui.colorScheme = localStorage.getItem('toodue-color-scheme') || 'sky';
  applyTheme();
  matchMedia('(prefers-color-scheme: light)').addEventListener('change', applyTheme);
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  window.addEventListener('hashchange', syncRoute);
  window.addEventListener('online', markOnline);
  window.addEventListener('offline', markOffline);
  syncRoute();

  try {
    data.user = await api.get('/auth/me');
    data.offline = false;
  } catch (err) {
    if (isOfflineError(err) && restoreSnapshot()) return;
    data.user = null;
    return;
  }
  await afterSignIn();
}

export async function afterSignIn() {
  await refresh();
  connectSSE();
  data.ready = true;
  syncOfflineQueue().catch(() => {});
}

export async function refresh() {
  try {
    const [projects, tasks, templates] = await Promise.all([api.get('/projects'), api.get('/tasks'), api.get('/templates')]);
    data.projects = projects;
    applyServerTasks(tasks);
    data.templates = templates;
    data.offline = false;
    saveCurrentSnapshot();
  } catch (err) {
    if (isOfflineError(err) && restoreSnapshot()) return;
    throw err;
  }
}

// Replace the task list with the server's, keeping optimistic state: rows for creates still in
// flight, pending edits re-applied on top of fresh confirmed state, and local deletions hidden.
function applyServerTasks(tasks) {
  const inflight = data.tasks.filter((t) => pendingCreates.has(t.id));
  data.tasks = [...tasks.filter((t) => !deletedTaskIds.has(t.id)), ...inflight];
  for (const [id, edit] of taskEdits) {
    const fresh = tasks.find((t) => t.id === id);
    if (fresh) edit.base = fresh;
    projectTask(id);
  }
}

export async function refreshTasks() {
  try {
    applyServerTasks(await api.get('/tasks'));
    data.offline = false;
    saveCurrentSnapshot();
  } catch (err) {
    if (!isOfflineError(err)) throw err;
    markOffline();
  }
}

export async function refreshProjects() {
  try {
    data.projects = await api.get('/projects');
    data.offline = false;
    saveCurrentSnapshot();
  } catch (err) {
    if (!isOfflineError(err)) throw err;
    markOffline();
  }
}

export async function refreshTemplates() {
  if (!requireOnlineAction()) return;
  data.templates = await api.get('/templates');
}

/* ---------- realtime ---------- */

let es;

export function connectSSE() {
  if (data.offline) return;
  if (es) es.close();
  es = new EventSource(apiUrl('/events', { withToken: true }));
  es.onopen = () => {
    data.offline = false;
  };
  es.onerror = () => {
    if (typeof navigator !== 'undefined' && navigator.onLine === false) markOffline();
  };
  es.onmessage = (e) => {
    if (!e.data || e.data === 'ping') return;
    let ev;
    try {
      ev = JSON.parse(e.data);
    } catch {
      return;
    }
    handleEvent(ev);
    bus.dispatchEvent(new CustomEvent('server-event', { detail: ev }));
  };
}

export function disconnectSSE() {
  if (es) es.close();
  es = null;
}

function upsert(list, item) {
  const i = list.findIndex((x) => x.id === item.id);
  if (i >= 0) list[i] = item;
  else list.push(item);
}

function handleEvent({ type, data: d }) {
  switch (type) {
    case 'task.upsert':
      // The main list only holds incomplete tasks; detail views listen on the bus.
      reconcileTask(d);
      break;
    case 'task.remove':
      // Another client deleted it: make sure no pending edit here can bring it back.
      for (const t of data.tasks) {
        if (t.id === d.id || t.parent_id === d.id) {
          deletedTaskIds.add(t.id);
          taskEdits.delete(t.id);
        }
      }
      for (const [cid, e] of taskEdits) {
        if (e.base.parent_id === d.id) {
          deletedTaskIds.add(cid);
          taskEdits.delete(cid);
        }
      }
      deletedTaskIds.add(d.id);
      taskEdits.delete(d.id);
      data.tasks = data.tasks.filter((t) => t.id !== d.id && t.parent_id !== d.id);
      if (ui.openTaskId === d.id) ui.openTaskId = null;
      break;
    case 'tasks.refresh':
      refreshTasks().catch(() => {});
      break;
    case 'projects.refresh':
      refreshProjects().catch(() => {});
      break;
    case 'project.upsert': {
      const known = data.projects.some((p) => p.id === d.id);
      upsert(data.projects, d);
      if (!known) refreshTasks().catch(() => {});
      break;
    }
    case 'project.remove':
      data.projects = data.projects.filter((p) => p.id !== d.id);
      // Pending edits on the project's tasks must not bring them back.
      for (const t of data.tasks) if (t.project_id === d.id) deletedTaskIds.add(t.id);
      for (const [tid, e] of taskEdits) {
        if (e.base.project_id === d.id) {
          deletedTaskIds.add(tid);
          taskEdits.delete(tid);
        }
      }
      data.tasks = data.tasks.filter((t) => t.project_id !== d.id);
      if (ui.view === 'project' && ui.projectId === d.id) navigate('projects');
      break;
  }
  saveCurrentSnapshot();
}

/* ---------- selectors ---------- */

export function inboxProject() {
  return data.projects.find((p) => p.is_inbox);
}

export function projectById(id) {
  return data.projects.find((p) => p.id === id);
}

/* ---------- actions ---------- */

export async function syncOfflineQueue() {
  if (data.syncPending || !hasQueuedOperations()) return;
  if (typeof navigator !== 'undefined' && navigator.onLine === false) {
    markOffline();
    return;
  }
  data.syncPending = true;
  try {
    const result = await replayQueuedOperations({ request, refresh });
    data.offline = false;
    connectSSE();
    if (result.replayed) toast('Offline changes synced.');
  } catch (err) {
    if (isOfflineError(err)) markOffline();
    else toast(`Could not sync offline changes: ${err.message}`);
    throw err;
  } finally {
    data.syncPending = false;
  }
}

/*
 * Mutations apply to local state immediately (optimistic), then reconcile with the server
 * response. On a network failure the change is queued for later sync; on any other error the
 * local change is rolled back and the error is rethrown so callers can toast it.
 */

// Temp id -> promise of the real task, for creates still in flight. A patch or delete on a
// just-added task waits for the server id instead of hitting /tasks/<temp id>.
const pendingCreates = new Map();
// Temp id -> real id for creates that have completed (delayed callbacks may still hold the temp id).
const createdIds = new Map();

// Ids deleted optimistically (or confirmed deleted); a late edit must not resurrect them.
const deletedTaskIds = new Set();

// Whether a task was deleted locally (pending or confirmed) this session.
export function isTaskDeleted(id) {
  return deletedTaskIds.has(id);
}

export async function resolveId(id) {
  if (createdIds.has(id)) return createdIds.get(id);
  const pending = pendingCreates.get(id);
  if (!pending) return id;
  const real = await pending;
  return real ? real.id : id;
}

/*
 * Per-task edit pipeline. Each task keeps its last confirmed state plus the edits still in
 * flight; the visible row is always confirmed + pending edits, so a response or failure for
 * one edit never clobbers or resurrects another. Requests for one task run in order.
 */
const taskEdits = new Map(); // id -> { base, pending: [{ fields }], chain: Promise }

// Bumped on sign-out so continuations of the previous user's mutations become no-ops.
let sessionGen = 0;

function resetMutationState() {
  sessionGen++;
  pendingCreates.clear();
  createdIds.clear();
  deletedTaskIds.clear();
  taskEdits.clear();
}

// Server state for a task arrived (event or refresh): it becomes the confirmed base and the
// pending edits are re-applied on top, so an earlier edit's broadcast can't undo a later one.
function reconcileTask(t) {
  if (deletedTaskIds.has(t.id)) return;
  const edit = taskEdits.get(t.id);
  if (edit) {
    edit.base = t;
    projectTask(t.id);
  } else if (t.completed_at) {
    data.tasks = data.tasks.filter((x) => x.id !== t.id);
  } else {
    upsert(data.tasks, t);
  }
}

function projectTask(id) {
  const edit = taskEdits.get(id);
  if (!edit) return;
  let t = edit.base;
  for (const op of edit.pending) t = applyLocalTaskPatch(t, op.fields);
  if (deletedTaskIds.has(id)) return;
  if (t.completed_at) data.tasks = data.tasks.filter((x) => x.id !== id);
  else upsert(data.tasks, t);
}

export async function addTask(fields) {
  const gen = sessionGen;
  if (fields.parent_id != null) fields = { ...fields, parent_id: await resolveId(fields.parent_id) };
  if (gen !== sessionGen) throw new Error('Signed out');
  const taskFields = { ...fields, project_id: fields.project_id ?? inboxProject()?.id };
  const temp = createOfflineTask(taskFields, { userId: data.user?.id });
  upsert(data.tasks, temp);
  saveCurrentSnapshot();

  let settle;
  pendingCreates.set(temp.id, new Promise((r) => (settle = r)));
  try {
    const t = await api.post('/tasks', fields);
    if (gen !== sessionGen) {
      settle(null);
      return t;
    }
    // The SSE event may already have inserted the real row; replace the temp one either way.
    data.tasks = data.tasks.filter((x) => x.id !== temp.id);
    const gone = deletedTaskIds.has(temp.id) || deletedTaskIds.has(t.id) || (t.parent_id != null && deletedTaskIds.has(t.parent_id));
    if (gone) deletedTaskIds.add(t.id);
    else upsert(data.tasks, t);
    if (ui.openTaskId === temp.id) ui.openTaskId = t.id;
    createdIds.set(temp.id, t.id);
    saveCurrentSnapshot();
    settle(t);
    return t;
  } catch (err) {
    if (gen !== sessionGen) {
      settle(null);
      throw err;
    }
    if (!mutationOffline(err)) {
      data.tasks = data.tasks.filter((x) => x.id !== temp.id);
      saveCurrentSnapshot();
      settle(null);
      throw err;
    }
    if (deletedTaskIds.has(temp.id) || (temp.parent_id != null && deletedTaskIds.has(temp.parent_id))) {
      settle(null);
      return temp;
    }
    upsert(data.tasks, temp); // a refresh may have dropped the row while the POST was pending
    queueTaskCreate(temp);
    saveCurrentSnapshot();
    offlineToast();
    settle(temp);
    return temp;
  } finally {
    if (gen === sessionGen) pendingCreates.delete(temp.id);
  }
}

export async function updateTask(id, fields) {
  const gen = sessionGen;
  id = await resolveId(id);
  if (gen !== sessionGen) throw new Error('Signed out');
  let edit = taskEdits.get(id);
  const existing = edit ? edit.base : data.tasks.find((task) => task.id === id);
  if (!existing) {
    // Not in the main list (e.g. a completed task): nothing to show optimistically.
    const t = await api.patch(`/tasks/${id}`, fields);
    if (gen !== sessionGen) return t;
    reconcileTask(t);
    saveCurrentSnapshot();
    return t;
  }

  if (!edit) {
    edit = { base: existing, pending: [], chain: Promise.resolve() };
    taskEdits.set(id, edit);
  }
  const op = { fields };
  edit.pending.push(op);
  projectTask(id);

  const settle = () => {
    edit.pending = edit.pending.filter((x) => x !== op);
    projectTask(id);
    // Keep the confirmed state around while a delete is pending so its rollback can use it.
    if (!edit.pending.length && !deletedTaskIds.has(id)) taskEdits.delete(id);
    saveCurrentSnapshot();
  };

  const run = async () => {
    if (gen !== sessionGen) throw new Error('Signed out');
    try {
      const t = await api.patch(`/tasks/${id}`, fields);
      if (gen !== sessionGen) return t;
      edit.base = t;
      settle();
      if (fields.completed === true && (existing.repeat_rule || t.repeat_rule)) {
        await refreshTasks();
      }
      return t;
    } catch (err) {
      if (gen !== sessionGen) throw err;
      if (!mutationOffline(err)) {
        settle(); // drop just this edit; the confirmed state and other pending edits stay
        throw err;
      }
      // Queued for sync: treat it as applied to the confirmed state so it stays visible.
      edit.base = applyLocalTaskPatch(edit.base, fields);
      if (!deletedTaskIds.has(id)) queueTaskPatch(id, fields); // a queued delete would 404 it
      settle();
      offlineToast();
      return edit.base;
    }
  };
  const result = edit.chain.catch(() => {}).then(run);
  edit.chain = result;
  return result;
}

export async function completeTask(task, completed) {
  try {
    await updateTask(task.id, {
      completed,
      ...(completed ? { completed_on: todayStr() } : {})
    });
  } catch (e) {
    toast(e.message);
  }
}

export async function deleteTask(id) {
  const gen = sessionGen;
  id = await resolveId(id);
  if (gen !== sessionGen) throw new Error('Signed out');
  // Children hidden by an optimistic completion live only in their edit pipeline; include them.
  const hiddenChildren = [...taskEdits.entries()]
    .filter(([cid, e]) => e.base.parent_id === id && !data.tasks.some((t) => t.id === cid))
    .map(([cid, e]) => e.base);
  const removed = [...data.tasks.filter((t) => t.id === id || t.parent_id === id), ...hiddenChildren];
  deletedTaskIds.add(id);
  for (const t of removed) deletedTaskIds.add(t.id);
  data.tasks = data.tasks.filter((t) => t.id !== id && t.parent_id !== id);
  if (ui.openTaskId === id) ui.openTaskId = null;
  if (ui.swipedTaskId === id) ui.swipedTaskId = null;
  // Views holding their own task lists (e.g. a project's completed tasks) drop the row here.
  bus.dispatchEvent(new CustomEvent('task-deleted', { detail: { id } }));
  try {
    await api.del(`/tasks/${id}`);
    if (gen !== sessionGen) return;
  } catch (err) {
    if (gen !== sessionGen) throw err;
    if (!mutationOffline(err)) {
      // Restore the rows that were removed optimistically, unless a server event replaced them.
      deletedTaskIds.delete(id);
      for (const t of removed) deletedTaskIds.delete(t.id);
      const current = new Set(data.tasks.map((t) => t.id));
      const ids = new Set([id, ...removed.map((t) => t.id)]);
      for (const tid of ids) {
        if (current.has(tid)) continue;
        // A row with edits in flight comes back from its pipeline (confirmed + pending), not the stale copy.
        if (taskEdits.has(tid)) projectTask(tid);
        else {
          const copy = removed.find((t) => t.id === tid);
          if (copy) data.tasks.push(copy);
        }
      }
      for (const tid of ids) {
        const e = taskEdits.get(tid);
        if (e && !e.pending.length) taskEdits.delete(tid);
      }
      saveCurrentSnapshot();
      bus.dispatchEvent(new CustomEvent('task-delete-failed', { detail: { id } }));
      throw err;
    }
    queueTaskDelete(id);
    offlineToast();
  }
  for (const t of removed) taskEdits.delete(t.id);
  saveCurrentSnapshot();
}

export async function addProject(fields) {
  try {
    const p = await api.post('/projects', fields);
    upsert(data.projects, p);
    saveCurrentSnapshot();
    return p;
  } catch (err) {
    if (!mutationOffline(err)) throw err;
    const p = createOfflineProject(fields, { userId: data.user?.id });
    queueProjectCreate(p);
    upsert(data.projects, p);
    saveCurrentSnapshot();
    offlineToast();
    return p;
  }
}

export async function importTemplate(templateId, fields = {}) {
  if (!requireOnlineAction()) throw new Error('Connection required');
  const result = await api.post(`/templates/${encodeURIComponent(templateId)}/import`, fields);
  upsert(data.projects, result.project);
  await refreshTasks();
  saveCurrentSnapshot();
  return result;
}

export async function saveProjectAsTemplate(projectId, fields = {}) {
  if (!requireOnlineAction()) throw new Error('Connection required');
  const template = await api.post('/templates', { project_id: projectId, ...fields });
  const i = data.templates.findIndex((t) => t.id === template.id);
  if (i >= 0) data.templates[i] = template;
  else data.templates = [...data.templates, template];
  return template;
}

export async function updateProject(id, fields) {
  try {
    const p = await api.patch(`/projects/${id}`, fields);
    upsert(data.projects, p);
    saveCurrentSnapshot();
    return p;
  } catch (err) {
    if (!mutationOffline(err)) throw err;
    const project = data.projects.find((p) => p.id === id);
    if (!project) throw err;
    const local = { ...project, ...fields };
    queueProjectPatch(id, fields);
    upsert(data.projects, local);
    saveCurrentSnapshot();
    offlineToast();
    return local;
  }
}

export async function deleteProject(id) {
  try {
    await api.del(`/projects/${id}`);
  } catch (err) {
    if (!mutationOffline(err)) throw err;
    queueProjectDelete(id);
    offlineToast();
  }
  data.projects = data.projects.filter((p) => p.id !== id);
  data.tasks = data.tasks.filter((t) => t.project_id !== id);
  if (ui.view === 'project' && ui.projectId === id) navigate('projects');
  saveCurrentSnapshot();
}

export async function reorderProjects(items) {
  // Optimistic: apply the new tree locally, reconcile with the server after.
  const byId = new Map(items.map((i) => [i.id, i]));
  for (const p of data.projects) {
    const it = byId.get(p.id);
    if (it) {
      p.parent_id = it.parent_id;
      p.sort_order = it.sort_order;
    }
  }
  data.projects.sort(
    (a, b) => b.is_inbox - a.is_inbox || a.sort_order - b.sort_order || a.id - b.id
  );
  saveCurrentSnapshot();
  try {
    await api.post('/projects/reorder', { items });
  } catch (e) {
    if (mutationOffline(e)) {
      for (const item of items) queueProjectPatch(item.id, item);
      offlineToast();
      return;
    }
    toast(e.message);
    await refreshProjects();
  }
}

export async function shareProject(id, email) {
  if (!requireOnlineAction()) throw new Error('Connection required');
  const p = await api.post(`/projects/${id}/share`, { email });
  upsert(data.projects, p);
  saveCurrentSnapshot();
  return p;
}

export async function shareProjects(projectIds, email) {
  if (!requireOnlineAction()) throw new Error('Connection required');
  const result = await api.post('/projects/share-bulk', { email, project_ids: projectIds });
  for (const p of result.projects ?? []) upsert(data.projects, p);
  saveCurrentSnapshot();
  return result;
}

export async function removeMember(projectId, userId) {
  if (!requireOnlineAction()) throw new Error('Connection required');
  const p = await api.del(`/projects/${projectId}/members/${userId}`);
  if (userId === data.user.id) {
    data.projects = data.projects.filter((x) => x.id !== projectId);
    data.tasks = data.tasks.filter((t) => t.project_id !== projectId);
    if (ui.view === 'project' && ui.projectId === projectId) navigate('projects');
  } else {
    upsert(data.projects, p);
  }
  saveCurrentSnapshot();
}

export async function signOut() {
  try {
    await api.post('/auth/logout');
  } catch {}
  setSessionToken(null);
  disconnectSSE();
  clearOfflineState();
  resetMutationState();
  data.user = null;
  data.projects = [];
  data.tasks = [];
  data.ready = false;
  data.offline = false;
  data.syncPending = false;
  navigate('today');
}
