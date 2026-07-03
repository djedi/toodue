import test from 'node:test';
import assert from 'node:assert/strict';
import {
  createMemoryStorage,
  loadSnapshot,
  saveSnapshot,
  createOfflineTask,
  queueTaskCreate,
  queueTaskPatch,
  queueTaskDelete,
  replayQueuedOperations
} from './offlineStore.js';

test('snapshot persists user, projects, and tasks for offline boot', () => {
  const storage = createMemoryStorage();
  const snapshot = {
    user: { id: 7, name: 'Dustin', email: 'dustin@example.com' },
    projects: [{ id: 1, name: 'Inbox', is_inbox: 1 }],
    tasks: [{ id: 11, project_id: 1, name: 'Cached task' }]
  };

  saveSnapshot(snapshot, storage);

  assert.deepEqual(loadSnapshot(storage), snapshot);
});

test('queueTaskCreate creates a local task and replay posts it before refreshing', async () => {
  const storage = createMemoryStorage();
  const localTask = createOfflineTask(
    { project_id: 1, name: 'Buy milk', due_date: '2026-07-03' },
    { userId: 7, now: '2026-07-03T12:00:00.000Z', tempId: -1 }
  );

  queueTaskCreate(localTask, storage);

  const calls = [];
  await replayQueuedOperations(
    {
      request: async (method, path, body) => {
        calls.push({ method, path, body });
        return { id: 99, ...body };
      },
      refresh: async () => calls.push({ method: 'REFRESH' })
    },
    storage
  );

  assert.deepEqual(calls, [
    {
      method: 'POST',
      path: '/tasks',
      body: { project_id: 1, name: 'Buy milk', due_date: '2026-07-03' }
    },
    { method: 'REFRESH' }
  ]);
  assert.equal(storage.getItem('toodue.offline.queue.v1'), '[]');
});

test('patching an unsynced local task merges into the pending create instead of queueing PATCH', () => {
  const storage = createMemoryStorage();
  const localTask = createOfflineTask(
    { project_id: 1, name: 'Draft', priority: 4 },
    { userId: 7, now: '2026-07-03T12:00:00.000Z', tempId: -5 }
  );
  queueTaskCreate(localTask, storage);

  queueTaskPatch(-5, { name: 'Draft edited', priority: 1 }, storage);

  assert.deepEqual(JSON.parse(storage.getItem('toodue.offline.queue.v1')), [
    {
      type: 'task.create',
      tempId: -5,
      body: { project_id: 1, name: 'Draft edited', priority: 1 }
    }
  ]);
});

test('deleting an unsynced local task removes the pending create', () => {
  const storage = createMemoryStorage();
  const localTask = createOfflineTask(
    { project_id: 1, name: 'Never mind' },
    { userId: 7, now: '2026-07-03T12:00:00.000Z', tempId: -2 }
  );
  queueTaskCreate(localTask, storage);

  queueTaskDelete(-2, storage);

  assert.equal(storage.getItem('toodue.offline.queue.v1'), '[]');
});

test('completing an unsynced local task removes the pending create', () => {
  const storage = createMemoryStorage();
  const localTask = createOfflineTask(
    { project_id: 1, name: 'Already done' },
    { userId: 7, now: '2026-07-03T12:00:00.000Z', tempId: -3 }
  );
  queueTaskCreate(localTask, storage);

  queueTaskPatch(-3, { completed: true }, storage);

  assert.equal(storage.getItem('toodue.offline.queue.v1'), '[]');
});

test('replay stops and preserves queue when a request fails', async () => {
  const storage = createMemoryStorage();
  queueTaskPatch(10, { completed: true }, storage);

  await assert.rejects(
    replayQueuedOperations(
      {
        request: async () => {
          throw new Error('still offline');
        },
        refresh: async () => {
          throw new Error('should not refresh');
        }
      },
      storage
    ),
    /still offline/
  );

  assert.deepEqual(JSON.parse(storage.getItem('toodue.offline.queue.v1')), [
    { type: 'task.patch', id: 10, body: { completed: true } }
  ]);
});
