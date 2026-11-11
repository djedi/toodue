import { test } from 'node:test';
import assert from 'node:assert/strict';
import { completedOnDayFilter, completedQuery, matchesCompletedFilter, purgeMessage } from './completed.js';

test('completedOnDayFilter spans the whole local day', () => {
  const { completed_from, completed_to } = completedOnDayFilter('2026-10-24');
  assert.equal(new Date(completed_from).getTime(), new Date(2026, 9, 24).getTime());
  assert.equal(new Date(completed_to).getTime(), new Date(2026, 9, 25).getTime() - 1);
  assert.match(completed_from, /Z$/);
});

test('completedQuery drops empty values and always asks for completed tasks', () => {
  const qs = completedQuery({ project_id: 7, due_from: '', completed_to: null });
  assert.equal(qs, 'completed=true&project_id=7');
  assert.equal(completedQuery(), 'completed=true');
});

test('matchesCompletedFilter applies every bound', () => {
  const task = {
    completed_at: '2026-10-24T15:00:00.000Z',
    project_id: 7,
    due_date: '2026-10-25'
  };
  assert.equal(matchesCompletedFilter(task, {}), true);
  assert.equal(matchesCompletedFilter({ ...task, completed_at: null }, {}), false);
  assert.equal(matchesCompletedFilter(task, { project_id: 7 }), true);
  assert.equal(matchesCompletedFilter(task, { project_id: '7' }), true);
  assert.equal(matchesCompletedFilter(task, { project_id: 8 }), false);
  assert.equal(
    matchesCompletedFilter(task, {
      completed_from: '2026-10-24T00:00:00.000Z',
      completed_to: '2026-10-24T23:59:59.999Z'
    }),
    true
  );
  assert.equal(matchesCompletedFilter(task, { completed_to: '2026-10-24T14:59:59.999Z' }), false);
  assert.equal(matchesCompletedFilter(task, { due_from: '2026-10-25' }), true);
  assert.equal(matchesCompletedFilter(task, { due_from: '2026-10-26' }), false);
  assert.equal(matchesCompletedFilter({ ...task, due_date: null }, { due_from: '2026-10-01' }), false);
});

test('purgeMessage reports deleted and skipped counts', () => {
  assert.equal(purgeMessage({ deleted: 0 }), 'No completed tasks to delete');
  assert.equal(purgeMessage({ deleted: 1 }), 'Deleted 1 completed task');
  assert.equal(
    purgeMessage({ deleted: 3, skipped: 1 }),
    "Deleted 3 completed tasks; kept 1 with subtasks in projects you can't access"
  );
});
