import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('task detail offers a leading Back action instead of a trailing close icon', () => {
  const source = readFileSync(new URL('./components/TaskModal.svelte', import.meta.url), 'utf8');
  assert.match(source, /aria-label="Back to task list"\s+onclick=\{close\}/);
  assert.match(source, /<ArrowLeft size=\{20\} aria-hidden="true" \/>\s+Back/);
  assert.ok(source.indexOf('aria-label="Back to task list"') < source.indexOf('aria-label="Delete task"'));
  assert.doesNotMatch(source, /aria-label="Close"|<X\s/);
  assert.match(source, /function close\(\) \{\s+ui\.openTaskId = null;/);
});
