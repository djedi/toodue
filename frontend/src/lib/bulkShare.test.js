import test from 'node:test';
import assert from 'node:assert/strict';
import { bulkShareMessage } from './bulkShare.js';

test('bulkShareMessage summarizes shared, already shared, and skipped projects', () => {
  assert.equal(
    bulkShareMessage({
      email: 'friend@example.com',
      shared: [1, 2],
      already_shared: [3],
      skipped: [{ id: 4, reason: 'the inbox cannot be shared' }]
    }),
    'Shared 2 projects with friend@example.com. 1 was already shared. 1 skipped.'
  );
});

test('bulkShareMessage handles no new shares', () => {
  assert.equal(
    bulkShareMessage({ email: 'friend@example.com', shared: [], already_shared: [3], skipped: [] }),
    'No new projects shared. 1 was already shared.'
  );
});
