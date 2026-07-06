import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = readFileSync(new URL('./components/Auth.svelte', import.meta.url), 'utf8');

test('logged-out auth page keeps login focused and minimal', () => {
  assert.match(source, /Welcome back/);
  assert.match(source, /Log in/);
  assert.match(source, /Sign up/);
  assert.doesNotMatch(source, /ThemeSwitcher/);
  assert.doesNotMatch(source, /ColorSchemePicker/);
  assert.doesNotMatch(source, /API docs/);
  assert.doesNotMatch(source, /docs\.toodue\.com/);
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

test('color scheme picker keeps the app accent options available from settings', () => {
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
