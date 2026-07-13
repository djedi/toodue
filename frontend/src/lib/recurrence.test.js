import test from 'node:test';
import assert from 'node:assert/strict';
import { repeatOptions, repeatLabel, normalizeRepeatRule } from './recurrence.js';

test('repeat choices cover the supported API rules', () => {
  assert.deepEqual(
    repeatOptions.map((option) => option.value),
    ['', 'daily', 'weekly', 'monthly', 'yearly']
  );
});

test('repeat labels are friendly and tolerate missing values', () => {
  assert.equal(repeatLabel('weekly'), 'Every week');
  assert.equal(repeatLabel(null), 'Does not repeat');
});

test('repeat rule is cleared when a task has no due date', () => {
  assert.equal(normalizeRepeatRule('daily', ''), null);
  assert.equal(normalizeRepeatRule('monthly', '2026-07-13'), 'monthly');
});
