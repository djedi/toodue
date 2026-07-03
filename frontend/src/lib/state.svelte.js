import { api, setUnauthorizedHandler } from './api.js';

export const data = $state({
  user: undefined, // undefined = booting, null = signed out
  projects: [],
  tasks: [],
  ready: false
});

export const ui = $state({
  view: 'today',
  projectId: null,
  openTaskId: null,
  quickAdd: null, // null or { project_id?, due_date?, parent_id? }
  showSettings: false,
  toast: null,
  theme: 'system'
});

// Components (e.g. the task modal) listen here for raw server events.
export const bus = new EventTarget();

setUnauthorizedHandler(() => {
  if (data.user) data.user = null;
});

/* ---------- theme ---------- */

export function applyTheme() {
  const dark =
    ui.theme === 'dark' ||
    (ui.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', dark);
}

export function setTheme(t) {
  ui.theme = t;
  localStorage.setItem('toodue-theme', t);
  applyTheme();
}

/* ---------- toast ---------- */

let toastTimer;
export function toast(msg) {
  ui.toast = msg;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast = null), 3500);
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
  applyTheme();
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  window.addEventListener('hashchange', syncRoute);
  syncRoute();

  try {
    data.user = await api.get('/auth/me');
  } catch {
    data.user = null;
    return;
  }
  await afterSignIn();
}

export async function afterSignIn() {
  await refresh();
  connectSSE();
  data.ready = true;
}

export async function refresh() {
  const [projects, tasks] = await Promise.all([api.get('/projects'), api.get('/tasks')]);
  data.projects = projects;
  data.tasks = tasks;
}

export async function refreshTasks() {
  data.tasks = await api.get('/tasks');
}

export async function refreshProjects() {
  data.projects = await api.get('/projects');
}

/* ---------- realtime ---------- */

let es;

export function connectSSE() {
  if (es) es.close();
  es = new EventSource('/api/events');
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
}

/* ---------- selectors ---------- */

export function inboxProject() {
  return data.projects.find((p) => p.is_inbox);
}

export function projectById(id) {
  return data.projects.find((p) => p.id === id);
}

/* ---------- actions ---------- */

export async function addTask(fields) {
  const t = await api.post('/tasks', fields);
  upsert(data.tasks, t);
  return t;
}

export async function updateTask(id, fields) {
  const t = await api.patch(`/tasks/${id}`, fields);
  if (t.completed_at) data.tasks = data.tasks.filter((x) => x.id !== t.id);
  else upsert(data.tasks, t);
  return t;
}

export async function completeTask(task, completed) {
  // Optimistic: flip locally, revert on failure.
  const idx = data.tasks.findIndex((t) => t.id === task.id);
  const prev = idx >= 0 ? data.tasks[idx].completed_at : task.completed_at;
  if (completed && idx >= 0) data.tasks.splice(idx, 1);
  try {
    await api.patch(`/tasks/${task.id}`, { completed });
  } catch (e) {
    if (completed && idx >= 0) data.tasks.splice(idx, 0, { ...task, completed_at: prev });
    toast(e.message);
  }
}

export async function deleteTask(id) {
  await api.del(`/tasks/${id}`);
  data.tasks = data.tasks.filter((t) => t.id !== id && t.parent_id !== id);
  if (ui.openTaskId === id) ui.openTaskId = null;
}

export async function addProject(fields) {
  const p = await api.post('/projects', fields);
  upsert(data.projects, p);
  return p;
}

export async function updateProject(id, fields) {
  const p = await api.patch(`/projects/${id}`, fields);
  upsert(data.projects, p);
  return p;
}

export async function deleteProject(id) {
  await api.del(`/projects/${id}`);
  data.projects = data.projects.filter((p) => p.id !== id);
  data.tasks = data.tasks.filter((t) => t.project_id !== id);
  if (ui.view === 'project' && ui.projectId === id) navigate('projects');
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
  try {
    await api.post('/projects/reorder', { items });
  } catch (e) {
    toast(e.message);
    await refreshProjects();
  }
}

export async function shareProject(id, email) {
  const p = await api.post(`/projects/${id}/share`, { email });
  upsert(data.projects, p);
  return p;
}

export async function shareProjects(projectIds, email) {
  const result = await api.post('/projects/share-bulk', { email, project_ids: projectIds });
  for (const p of result.projects ?? []) upsert(data.projects, p);
  return result;
}

export async function removeMember(projectId, userId) {
  const p = await api.del(`/projects/${projectId}/members/${userId}`);
  if (userId === data.user.id) {
    data.projects = data.projects.filter((x) => x.id !== projectId);
    data.tasks = data.tasks.filter((t) => t.project_id !== projectId);
    if (ui.view === 'project' && ui.projectId === projectId) navigate('projects');
  } else {
    upsert(data.projects, p);
  }
}

export async function signOut() {
  try {
    await api.post('/auth/logout');
  } catch {}
  disconnectSSE();
  data.user = null;
  data.projects = [];
  data.tasks = [];
  data.ready = false;
  navigate('today');
}
