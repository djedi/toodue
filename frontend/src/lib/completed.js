import { addDays, toDateStr } from './dates.js';

// Which completed tasks a view shows. Mirrors the server's CompletedFilter: `project_id`,
// `completed_from`/`completed_to` (RFC 3339 UTC, inclusive), `due_from`/`due_to` (YYYY-MM-DD).

function localDayStart(dayStr) {
  const [y, m, d] = dayStr.split('-').map(Number);
  return new Date(y, m - 1, d);
}

// Completed during the given local calendar day.
export function completedOnDayFilter(dayStr) {
  const start = localDayStart(dayStr);
  const end = localDayStart(addDays(dayStr, 1));
  return {
    completed_from: start.toISOString(),
    completed_to: new Date(end.getTime() - 1).toISOString()
  };
}

export function completedQuery(filter) {
  const params = new URLSearchParams({ completed: 'true' });
  for (const [key, value] of Object.entries(filter ?? {})) {
    if (value !== undefined && value !== null && value !== '') params.set(key, String(value));
  }
  return params.toString();
}

// Whether a task (as the server returned it) belongs to the filter's completed set.
export function matchesCompletedFilter(task, filter = {}) {
  if (!task?.completed_at) return false;
  if (filter.project_id != null && task.project_id !== Number(filter.project_id)) return false;
  if (filter.completed_from && task.completed_at < filter.completed_from) return false;
  if (filter.completed_to && task.completed_at > filter.completed_to) return false;
  if (filter.due_from && !(task.due_date && task.due_date >= filter.due_from)) return false;
  if (filter.due_to && !(task.due_date && task.due_date <= filter.due_to)) return false;
  return true;
}

export function completedOnLocalDay(iso, dayStr) {
  return !!iso && toDateStr(new Date(iso)) === dayStr;
}

// Toast text for a DELETE /tasks/completed response.
export function purgeMessage({ deleted = 0, skipped = 0 } = {}) {
  let msg =
    deleted === 0 ? 'No completed tasks to delete' : deleted === 1 ? 'Deleted 1 completed task' : `Deleted ${deleted} completed tasks`;
  if (skipped > 0) {
    msg += `; kept ${skipped} with subtasks in projects you can't access`;
  }
  return msg;
}
