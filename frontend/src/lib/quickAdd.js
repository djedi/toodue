import { parseNatural, quickOptions } from './datePicker.js';
import { todayStr } from './dates.js';
import { repeatOptions } from './recurrence.js';

// Only the unambiguous, standalone !!1–!!4 syntax is automatic.
export function parseQuickAdd(text) {
  let priority;
  const stripped = text.replace(/"(?:\\.|[^"\\])*"|(^|\s)!!([1-4])(?=\s|$)/g, (match, space, value) => {
    if (!value) return match; // Quoted text is literal.
    priority = Number(value);
    return space;
  });
  const name = (priority === undefined ? stripped : stripped.replace(/\s{2,}/g, ' ')).trim();
  return priority === undefined ? { name } : { name, priority };
}

export function activeShortcut(text, cursor) {
  const before = text.slice(0, cursor);
  const match = /(?:^|\s)(#|!!|\/)([^#!\n]*)$/.exec(before);
  if (!match) return null;
  const start = match.index + (match[0].startsWith(' ') || /^\s/.test(match[0]) ? 1 : 0);
  // Never replace the remainder of a partially typed word at a moved cursor.
  if (cursor < text.length && !/\s/.test(text[cursor])) return null;
  return { kind: match[1], query: match[2].replace(/^"|"$/g, ''), start, end: cursor };
}

export function shortcutOptions(token, projects, today = todayStr()) {
  if (!token) return [];
  const query = token.query.toLowerCase().trim();
  if (token.kind === '#') return projects.filter(p => (p.is_inbox ? 'Inbox' : p.name).toLowerCase().includes(query)).map(p => ({ label: p.is_inbox ? 'Inbox' : p.name, project_id: p.id }));
  if (token.kind === '!!') return [1, 2, 3, 4].filter(p => !query || String(p) === query).map(priority => ({ label: `Priority ${priority}`, priority }));
  const choices = quickOptions(today).map(o => ({ label: o.label, due_date: o.date }));
  const parsed = parseNatural(query, today);
  const dates = parsed ? [{ label: `Date · ${query}`, due_date: parsed }] : choices.filter(o => o.label.toLowerCase().includes(query));
  return [...dates, ...repeatOptions.filter(o => o.value && o.label.toLowerCase().includes(query)).map(o => ({ label: o.label, repeat_rule: o.value, due_date: today }))];
}

export function consumeShortcut(text, token) {
  const prefix = text.slice(0, token.start);
  const suffix = text.slice(token.end).replace(/^\s+/, '');
  return { name: prefix + suffix, cursor: prefix.length };
}

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
