import test from 'node:test';
import assert from 'node:assert/strict';
import {
  nextWeekday,
  quickOptions,
  monthGrid,
  monthStarts,
  monthLabel,
  parseNatural,
  weekdayAbbrev
} from './datePicker.js';

// 2026-07-06 is a Monday.
const TODAY = '2026-07-06';

test('nextWeekday finds the next occurrence strictly after the date', () => {
  assert.equal(nextWeekday(TODAY, 6), '2026-07-11'); // Mon → Sat
  assert.equal(nextWeekday(TODAY, 1), '2026-07-13'); // Mon → next Mon
  assert.equal(nextWeekday('2026-07-11', 6), '2026-07-18'); // Sat → next Sat
});

test('quickOptions covers today through next week plus no-date', () => {
  const options = quickOptions(TODAY);
  assert.deepEqual(
    options.map((o) => [o.key, o.date]),
    [
      ['today', '2026-07-06'],
      ['tomorrow', '2026-07-07'],
      ['weekend', '2026-07-11'],
      ['next-week', '2026-07-13'],
      ['no-date', null]
    ]
  );
});

test('monthGrid drops fully past weeks of the current month', () => {
  const weeks = monthGrid(2026, 7, TODAY);
  // July 2026 starts on Wednesday; the July 1–4 week contains no day >= the
  // 5th, so the grid starts with the week of Sun Jul 5.
  assert.equal(weeks[0][0], '2026-07-05');
  assert.equal(weeks.at(-1).filter(Boolean).at(-1), '2026-07-31');
  assert.ok(weeks.every((w) => w.length === 7));
});

test('monthGrid keeps future months intact', () => {
  const weeks = monthGrid(2026, 8, TODAY);
  assert.equal(weeks[0].filter(Boolean)[0], '2026-08-01');
});

test('monthStarts begins at the current month', () => {
  const months = monthStarts(13, TODAY);
  assert.deepEqual(months[0], { year: 2026, month: 7 });
  assert.deepEqual(months.at(-1), { year: 2027, month: 7 });
});

test('monthLabel includes the year only when it differs', () => {
  assert.equal(monthLabel(2026, 7, TODAY), 'July');
  assert.equal(monthLabel(2027, 1, TODAY), 'January 2027');
});

test('weekdayAbbrev', () => {
  assert.equal(weekdayAbbrev('2026-07-06'), 'Mon');
  assert.equal(weekdayAbbrev('2026-07-11'), 'Sat');
});

test('parseNatural handles keywords', () => {
  assert.equal(parseNatural('today', TODAY), '2026-07-06');
  assert.equal(parseNatural('Tomorrow', TODAY), '2026-07-07');
  assert.equal(parseNatural('next week', TODAY), '2026-07-13');
  assert.equal(parseNatural('weekend', TODAY), '2026-07-11');
});

test('parseNatural handles weekday names and prefixes', () => {
  assert.equal(parseNatural('fri', TODAY), '2026-07-10');
  assert.equal(parseNatural('next friday', TODAY), '2026-07-10');
  assert.equal(parseNatural('monday', TODAY), '2026-07-13');
});

test('parseNatural handles relative offsets', () => {
  assert.equal(parseNatural('in 3 days', TODAY), '2026-07-09');
  assert.equal(parseNatural('in 2 weeks', TODAY), '2026-07-20');
});

test('parseNatural handles explicit dates', () => {
  assert.equal(parseNatural('2026-12-25', TODAY), '2026-12-25');
  assert.equal(parseNatural('7/20', TODAY), '2026-07-20');
  assert.equal(parseNatural('7/20/2027', TODAY), '2027-07-20');
  assert.equal(parseNatural('jul 20', TODAY), '2026-07-20');
  assert.equal(parseNatural('20 jul', TODAY), '2026-07-20');
  assert.equal(parseNatural('december 25 2027', TODAY), '2027-12-25');
});

test('parseNatural rolls yearless past dates into next year', () => {
  assert.equal(parseNatural('1/5', TODAY), '2027-01-05');
  assert.equal(parseNatural('jan 5', TODAY), '2027-01-05');
});

test('parseNatural rejects garbage', () => {
  assert.equal(parseNatural('', TODAY), null);
  assert.equal(parseNatural('xx', TODAY), null);
  assert.equal(parseNatural('hello world', TODAY), null);
  assert.equal(parseNatural('13/45', TODAY), null);
  assert.equal(parseNatural('feb 30', TODAY), null);
});
