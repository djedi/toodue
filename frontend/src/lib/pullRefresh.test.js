import test from 'node:test';
import assert from 'node:assert/strict';
import { getPullRefreshState } from './pullRefresh.js';

test('getPullRefreshState ignores pulls when content is already scrolled', () => {
  assert.deepEqual(getPullRefreshState({ startY: 10, currentY: 90, scrollTop: 4 }), {
    distance: 0,
    ready: false
  });
});

test('getPullRefreshState applies resistance and marks ready at threshold', () => {
  assert.deepEqual(getPullRefreshState({ startY: 10, currentY: 150, scrollTop: 0, threshold: 70 }), {
    distance: 70,
    ready: true
  });
});

test('getPullRefreshState clamps the visual pull distance', () => {
  assert.deepEqual(getPullRefreshState({ startY: 0, currentY: 400, scrollTop: 0, threshold: 70, maxDistance: 96 }), {
    distance: 96,
    ready: true
  });
});
