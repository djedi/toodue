// What each OAuth scope lets a connected AI agent do. Order matches the server.
export const SCOPES = [
  {
    id: 'read',
    label: 'View your projects and tasks',
    detail: 'Read projects, tasks, sub-tasks, comments, and due dates',
    required: true
  },
  {
    id: 'write',
    label: 'Create and update tasks',
    detail: 'Add, edit, complete, and comment on tasks; create projects'
  },
  {
    id: 'delete',
    label: 'Delete tasks',
    detail: 'Permanently remove tasks and their sub-tasks'
  }
];

export function scopeLabels(scope) {
  const granted = typeof scope === 'string' ? scope.split(/\s+/) : scope ?? [];
  return SCOPES.filter((s) => granted.includes(s.id)).map((s) => s.label);
}

export function summarizeScope(scope) {
  const granted = typeof scope === 'string' ? scope.split(/\s+/) : scope ?? [];
  if (granted.includes('delete')) return 'Full access';
  if (granted.includes('write')) return 'Read & write';
  return 'Read only';
}

export function connectRequestId(loc) {
  if (loc.pathname !== '/connect') return null;
  return new URLSearchParams(loc.search).get('request') || null;
}

// Redirects to private-use schemes (e.g. cursor://) hand off to a desktop app
// and leave this tab open, so the page should say so.
export function isAppRedirect(url) {
  return !/^https?:\/\//i.test(url);
}
