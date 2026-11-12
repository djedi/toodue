import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DISPLAY_SIZE_KEY, DISPLAY_SIZES, normalizeDisplaySize, readDisplaySize, applyDisplaySize, saveDisplaySize } from './displaySize.js';

test('display size defaults and invalid values normalize to standard', () => {
  for (const value of [undefined, null, '', 'huge', 'Large', {}, 1]) {
    assert.equal(normalizeDisplaySize(value), 'standard');
  }
  for (const { value } of DISPLAY_SIZES) assert.equal(normalizeDisplaySize(value), value);
  assert.equal(readDisplaySize({ getItem: () => null }), 'standard');
  assert.equal(readDisplaySize({ getItem: () => 'invalid' }), 'standard');
});

test('each size persists locally, reloads, and applies immediately', () => {
  const values = new Map();
  const storage = { getItem: (key) => values.get(key), setItem: (key, value) => values.set(key, value) };
  const root = { dataset: { colorScheme: 'sky' } };
  for (const { value } of DISPLAY_SIZES) {
    assert.equal(saveDisplaySize(value, storage, root), value);
    assert.equal(values.get(DISPLAY_SIZE_KEY), value);
    assert.equal(readDisplaySize(storage), value);
    assert.equal(root.dataset.displaySize, value);
    assert.equal(root.dataset.colorScheme, 'sky');
    const reloadedRoot = { dataset: {} };
    applyDisplaySize(readDisplaySize(storage), reloadedRoot);
    assert.equal(reloadedRoot.dataset.displaySize, value);
  }
  assert.equal(saveDisplaySize('bad', storage, root), 'standard');
  assert.equal(readDisplaySize(storage), 'standard');
});

test('restricted storage still allows sizing and missing DOM is safe', () => {
  const storage = { getItem() { throw Error('blocked'); }, setItem() { throw Error('blocked'); } };
  assert.equal(readDisplaySize(storage), 'standard');
  const root = { dataset: {} };
  assert.equal(saveDisplaySize('large', storage, root), 'large');
  assert.equal(root.dataset.displaySize, 'large');
  assert.equal(applyDisplaySize('extra-large', null), 'extra-large');
});

test('bootstrap, mobile-only root scaling, scalable inputs, and Settings wiring remain connected', () => {
  const css = readFileSync(new URL('../app.css', import.meta.url), 'utf8');
  const main = readFileSync(new URL('../main.js', import.meta.url), 'utf8');
  const settings = readFileSync(new URL('./components/Settings.svelte', import.meta.url), 'utf8');
  assert.match(main, /applyDisplaySize\(readDisplaySize\(\)\)/);
  assert.match(main, /dataset.native = String\(isNative\)/);
  assert.match(css, /@media \(max-width: 767px\)\s*\{\s*html \{ font-size: var\(--mobile-font-size, 100%\); \}/);
  assert.match(css, /html\[data-native='true'\] \{ font-size: var\(--mobile-font-size, 100%\); \}/);
  assert.match(css, /font-size: max\(16px, 1rem\) !important/);
  assert.doesNotMatch(css, /\bzoom\s*:/);
  assert.match(settings, /saveDisplaySize\(size.value\)/);
  assert.match(settings, /type="radio"/);
  assert.match(settings, /flex flex-wrap gap-2/);
});
