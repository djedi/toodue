import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync(new URL('./components/Auth.svelte', import.meta.url), 'utf8');

test('logged-out homepage links to the public API docs', () => {
  assert.match(source, /href="https:\/\/docs\.toodue\.com"/);
  assert.match(source, />API docs</);
});
