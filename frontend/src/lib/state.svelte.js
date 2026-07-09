import { api, request, setUnauthorizedHandler, isOfflineError } from './api.js';
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
  quickAdd: null, // null or { project_id?, due_date?, parent_id? }
  showSettings: false,
  toast: null,
  theme: 'system',
  colorScheme: 'sky'
});

// Components (e.g. the task modal) listen here for raw server events.
export const bus = new EventTarget();

setUnauthorizedHandler(() => {
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
  for (const key of ['name', 'description', 'due_date', 'due_time', 'deadline', 'priority', 'project_id']) {
    if (key in fields) updated[key] = fields[key];
  }
  if (fields.due_date === null) updated.due_time = null;
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
    data.tasks = tasks;
    data.templates = templates;
    data.offline = false;
    saveCurrentSnapshot();
  } catch (err) {
    if (isOfflineError(err) && restoreSnapshot()) return;
    throw err;
  }
}

export async function refreshTasks() {
  try {
    data.tasks = await api.get('/tasks');
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
  es = new EventSource('/api/events');
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
      if (d.completed_at) data.tasks = data.tasks.filter((t) => t.id !== d.id);
      else upsert(data.tasks, d);
      break;
    case 'task.remove':
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

export async function addTask(fields) {
  try {
    const t = await api.post('/tasks', fields);
    upsert(data.tasks, t);
    saveCurrentSnapshot();
    return t;
  } catch (err) {
    if (!mutationOffline(err)) throw err;
    const taskFields = { ...fields, project_id: fields.project_id ?? inboxProject()?.id };
    const t = createOfflineTask(taskFields, { userId: data.user?.id });
    queueTaskCreate(t);
    upsert(data.tasks, t);
    saveCurrentSnapshot();
    offlineToast();
    return t;
  }
}

export async function updateTask(id, fields) {
  try {
    const t = await api.patch(`/tasks/${id}`, fields);
    if (t.completed_at) data.tasks = data.tasks.filter((x) => x.id !== t.id);
    else upsert(data.tasks, t);
    saveCurrentSnapshot();
    return t;
  } catch (err) {
    if (!mutationOffline(err)) throw err;
    const task = data.tasks.find((t) => t.id === id);
    if (!task) throw err;
    const local = applyLocalTaskPatch(task, fields);
    queueTaskPatch(id, fields);
    if (local.completed_at) data.tasks = data.tasks.filter((x) => x.id !== id);
    else upsert(data.tasks, local);
    saveCurrentSnapshot();
    offlineToast();
    return local;
  }
}

export async function completeTask(task, completed) {
  try {
    await updateTask(task.id, { completed });
  } catch (e) {
    toast(e.message);
  }
}

export async function deleteTask(id) {
  try {
    await api.del(`/tasks/${id}`);
  } catch (err) {
    if (!mutationOffline(err)) throw err;
    queueTaskDelete(id);
    offlineToast();
  }
  data.tasks = data.tasks.filter((t) => t.id !== id && t.parent_id !== id);
  if (ui.openTaskId === id) ui.openTaskId = null;
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
  disconnectSSE();
  clearOfflineState();
  data.user = null;
  data.projects = [];
  data.tasks = [];
  data.ready = false;
  data.offline = false;
  data.syncPending = false;
  navigate('today');
}
