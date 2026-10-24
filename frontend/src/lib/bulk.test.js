import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mapLimit } from './bulk.js';

test('mapLimit caps concurrency and keeps result order', async () => {
  let running = 0;
  let peak = 0;
  const results = await mapLimit([1, 2, 3, 4, 5, 6, 7], 3, async (n) => {
    running++;
    peak = Math.max(peak, running);
    await new Promise((r) => setTimeout(r, 5 - (n % 3)));
    running--;
    if (n === 4) throw new Error('nope');
    return n * 2;
  });
  assert.equal(peak, 3);
  assert.deepEqual(
    results.map((r) => (r.status === 'fulfilled' ? r.value : r.reason.message)),
    [2, 4, 6, 'nope', 10, 12, 14]
  );
});

test('mapLimit handles an empty list', async () => {
  assert.deepEqual(await mapLimit([], 4, async () => 1), []);
});
