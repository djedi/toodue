import { todayStr } from './dates.js';

// The local calendar day as reactive state, so views (and the date-scoped completed purges)
// move on at midnight and when the app comes back from the background.
let day = $state(todayStr());
let timer;

function sync() {
  day = todayStr();
  clearTimeout(timer);
  const now = new Date();
  const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  timer = setTimeout(sync, next.getTime() - now.getTime() + 1000);
}

if (typeof window !== 'undefined') {
  sync();
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') sync();
  });
}

export function currentDay() {
  return day;
}
