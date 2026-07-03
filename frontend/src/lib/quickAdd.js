export function getQuickAddDefaults({ view, projectId, today }) {
  const defaults = {};

  if (view === 'today') {
    defaults.due_date = today;
  }

  if (view === 'project' && projectId != null) {
    defaults.project_id = projectId;
  }

  return defaults;
}
