import test from 'node:test';
import assert from 'node:assert/strict';
import { getQuickAddDefaults } from './quickAdd.js';

test('getQuickAddDefaults sets due date to today when opened from Today view', () => {
  assert.deepEqual(getQuickAddDefaults({ view: 'today', today: '2026-07-03' }), {
    due_date: '2026-07-03'
  });
});

test('getQuickAddDefaults keeps project context without forcing a due date outside Today view', () => {
  assert.deepEqual(getQuickAddDefaults({ view: 'project', projectId: 42, today: '2026-07-03' }), {
    project_id: 42
  });
});
