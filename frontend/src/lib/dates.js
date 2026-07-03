function pad(n) {
  return String(n).padStart(2, '0');
}

export function toDateStr(d) {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function todayStr() {
  return toDateStr(new Date());
}

export function addDays(dateStr, n) {
  const [y, m, d] = dateStr.split('-').map(Number);
  const date = new Date(y, m - 1, d + n);
  return toDateStr(date);
}

export function isOverdue(dateStr) {
  return !!dateStr && dateStr < todayStr();
}

const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

export function dayLabel(dateStr) {
  const today = todayStr();
  if (dateStr === today) return 'Today';
  if (dateStr === addDays(today, 1)) return 'Tomorrow';
  if (dateStr === addDays(today, -1)) return 'Yesterday';
  const [y, m, d] = dateStr.split('-').map(Number);
  const date = new Date(y, m - 1, d);
  if (dateStr > today && dateStr <= addDays(today, 6)) return WEEKDAYS[date.getDay()];
  const monthDay = `${MONTHS[m - 1]} ${d}`;
  return y === new Date().getFullYear() ? monthDay : `${monthDay}, ${y}`;
}

export function fullDayLabel(dateStr) {
  const [y, m, d] = dateStr.split('-').map(Number);
  const date = new Date(y, m - 1, d);
  return `${dayLabel(dateStr)} · ${WEEKDAYS[date.getDay()].slice(0, 3)} ${MONTHS[m - 1]} ${d}`;
}

export function fmtTime(t) {
  if (!t) return '';
  const [h, m] = t.split(':').map(Number);
  const ampm = h >= 12 ? 'PM' : 'AM';
  const hr = h % 12 === 0 ? 12 : h % 12;
  return `${hr}:${pad(m)} ${ampm}`;
}

export function fmtTimestamp(iso) {
  const d = new Date(iso);
  return `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getHours() % 12 === 0 ? 12 : d.getHours() % 12}:${pad(d.getMinutes())} ${d.getHours() >= 12 ? 'PM' : 'AM'}`;
}
