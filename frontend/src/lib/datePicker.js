// Pure logic for the Todoist-style date picker: quick options, the month
// grid, and lightweight natural-language parsing. All dates are "YYYY-MM-DD"
// strings in local time (the API wire format).
import { toDateStr, todayStr, addDays } from './dates.js';

const WEEKDAY_NAMES = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];
const MONTH_NAMES = [
  'january', 'february', 'march', 'april', 'may', 'june',
  'july', 'august', 'september', 'october', 'november', 'december'
];

function parseDateStr(dateStr) {
  const [y, m, d] = dateStr.split('-').map(Number);
  return new Date(y, m - 1, d);
}

/** First date matching `weekday` (0 = Sunday … 6 = Saturday) strictly after `dateStr`. */
export function nextWeekday(dateStr, weekday) {
  const from = parseDateStr(dateStr);
  const delta = ((weekday - from.getDay() + 6) % 7) + 1;
  return addDays(dateStr, delta);
}

export function weekdayAbbrev(dateStr) {
  const name = WEEKDAY_NAMES[parseDateStr(dateStr).getDay()];
  return name[0].toUpperCase() + name.slice(1, 3);
}

/** Quick options for the picker; `date: null` means "No Date". */
export function quickOptions(today = todayStr()) {
  return [
    { key: 'today', label: 'Today', date: today },
    { key: 'tomorrow', label: 'Tomorrow', date: addDays(today, 1) },
    { key: 'weekend', label: 'This Weekend', date: nextWeekday(today, 6) },
    { key: 'next-week', label: 'Next Week', date: nextWeekday(today, 1) },
    { key: 'no-date', label: 'No Date', date: null }
  ];
}

/**
 * Weeks of a month as rows of 7 cells ("YYYY-MM-DD" or null padding).
 * Fully past weeks are dropped so the current month starts at this week.
 */
export function monthGrid(year, month, today = todayStr()) {
  const first = new Date(year, month - 1, 1);
  const daysInMonth = new Date(year, month, 0).getDate();
  const cells = Array(first.getDay()).fill(null);
  for (let d = 1; d <= daysInMonth; d++) {
    cells.push(toDateStr(new Date(year, month - 1, d)));
  }
  while (cells.length % 7 !== 0) cells.push(null);

  const weeks = [];
  for (let i = 0; i < cells.length; i += 7) {
    const week = cells.slice(i, i + 7);
    if (week.some((day) => day && day >= today)) weeks.push(week);
  }
  return weeks;
}

/** The picker's month list: current month plus the next `count - 1`. */
export function monthStarts(count = 13, today = todayStr()) {
  const [y, m] = today.split('-').map(Number);
  return Array.from({ length: count }, (_, i) => {
    const date = new Date(y, m - 1 + i, 1);
    return { year: date.getFullYear(), month: date.getMonth() + 1 };
  });
}

export function monthLabel(year, month, today = todayStr()) {
  const name = MONTH_NAMES[month - 1];
  const label = name[0].toUpperCase() + name.slice(1);
  return year === Number(today.slice(0, 4)) ? label : `${label} ${year}`;
}

/**
 * Lightweight natural-language parsing for the "Type a date" field:
 * today/tomorrow, weekday names, "in N days/weeks", and explicit dates
 * ("jul 20", "20 jul", "7/20", "2026-07-20"). Returns "YYYY-MM-DD" or null.
 */
export function parseNatural(input, today = todayStr()) {
  const s = input.trim().toLowerCase();
  if (s.length < 3) return null;

  if (s === 'today' || s === 'tod') return today;
  if (s === 'tomorrow' || s === 'tom' || s === 'tmr') return addDays(today, 1);
  if (s === 'next week') return nextWeekday(today, 1);
  if (s === 'weekend' || s === 'this weekend') return nextWeekday(today, 6);

  const relative = s.match(/^in (\d+) ?(day|days|d|week|weeks|w)$/);
  if (relative) {
    const n = Number(relative[1]);
    return addDays(today, relative[2].startsWith('w') ? n * 7 : n);
  }

  // Weekday names, optionally prefixed with "next" ("fri", "next friday").
  const name = s.startsWith('next ') ? s.slice(5) : s;
  if (name.length >= 3) {
    const weekday = WEEKDAY_NAMES.findIndex((w) => w.startsWith(name));
    if (weekday >= 0) return nextWeekday(today, weekday);
  }

  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) {
    return Number.isNaN(parseDateStr(s).getTime()) ? null : s;
  }

  // "7/20" or "7/20/2026".
  const slash = s.match(/^(\d{1,2})\/(\d{1,2})(?:\/(\d{2,4}))?$/);
  if (slash) {
    const month = Number(slash[1]);
    const day = Number(slash[2]);
    const year = slash[3] ? Number(slash[3].length === 2 ? `20${slash[3]}` : slash[3]) : null;
    return buildDate(month, day, year, today);
  }

  // "jul 20", "july 20", "20 jul", "jul 20 2026".
  const monthFirst = s.match(/^([a-z]{3,9}) (\d{1,2})(?:,? (\d{4}))?$/);
  const dayFirst = s.match(/^(\d{1,2}) ([a-z]{3,9})(?:,? (\d{4}))?$/);
  const match = monthFirst ?? dayFirst;
  if (match) {
    const monthName = monthFirst ? match[1] : match[2];
    const day = Number(monthFirst ? match[2] : match[1]);
    const month = MONTH_NAMES.findIndex((m) => m.startsWith(monthName)) + 1;
    if (month > 0) return buildDate(month, day, match[3] ? Number(match[3]) : null, today);
  }

  return null;
}

// Validate the components and, when no year is given, pick the next
// occurrence (this year, or next year if already past).
function buildDate(month, day, year, today) {
  const candidateYear = year ?? Number(today.slice(0, 4));
  const date = new Date(candidateYear, month - 1, day);
  if (date.getMonth() !== month - 1 || date.getDate() !== day) return null;
  const dateStr = toDateStr(date);
  if (year === null && dateStr < today) {
    return toDateStr(new Date(candidateYear + 1, month - 1, day));
  }
  return dateStr;
}
