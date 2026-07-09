import test from 'node:test';
import assert from 'node:assert/strict';
import { packingListTemplate, templatePreviewCount } from './templates.js';

test('packing list template has travel essentials and sensible grouping', () => {
  assert.equal(packingListTemplate.name, 'Packing List');
  assert.equal(packingListTemplate.color, 'sky');
  assert.ok(packingListTemplate.tasks.some((task) => task.name === 'Toiletries'));
  assert.ok(packingListTemplate.tasks.some((task) => task.name === 'Phone charger'));
  assert.ok(packingListTemplate.tasks.some((task) => task.name === 'Underwear'));
  assert.equal(templatePreviewCount(packingListTemplate), packingListTemplate.tasks.length);
});
