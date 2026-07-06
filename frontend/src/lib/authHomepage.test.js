import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync(new URL('./components/Auth.svelte', import.meta.url), 'utf8');

test('logged-out homepage links to the public API docs', () => {
  assert.match(source, /href="https:\/\/docs\.toodue\.com"/);
  assert.match(source, />API docs</);
});

test('logged-out homepage includes the light, dark, and system theme picker', () => {
  assert.match(source, /ThemeSwitcher/);
  assert.match(source, /Theme/);
});

test('logged-out homepage includes the color scheme picker', () => {
  assert.match(source, /ColorSchemePicker/);
  const colorSchemePicker = readFileSync(
    new URL('./components/ColorSchemePicker.svelte', import.meta.url),
    'utf8'
  );
  assert.match(colorSchemePicker, /Sky/);
  assert.match(colorSchemePicker, /Coral/);
  assert.match(colorSchemePicker, /Emerald/);
  assert.match(colorSchemePicker, /Violet/);
  assert.match(colorSchemePicker, /Amber/);
  assert.match(colorSchemePicker, /Rose/);
});

test('theme switcher uses Font Awesome icons in light, dark, system order', () => {
  const themeSwitcher = readFileSync(
    new URL('./components/ThemeSwitcher.svelte', import.meta.url),
    'utf8'
  );
  assert.match(themeSwitcher, /Font Awesome Free solid icons/);
  assert.match(themeSwitcher, /value: 'light'[\s\S]+value: 'dark'[\s\S]+value: 'system'/);
  assert.doesNotMatch(themeSwitcher, /@lucide\/svelte/);
});
