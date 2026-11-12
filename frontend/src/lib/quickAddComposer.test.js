import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const helpers = () => import('./quickAdd.js');
const projects = [{ id: 'a', name: 'Home Office' }, { id: 'b', name: 'Work' }];
test('only standalone explicit priorities consumed; last wins; stripped title can be empty', async () => {
 const { parseQuickAdd } = await helpers();
 assert.deepEqual(parseQuickAdd('Buy milk !!1 then !!3'), { name: 'Buy milk then', priority: 3 });
 assert.deepEqual(parseQuickAdd('!!1'), { name: '', priority: 1 });
 for (const name of ['C# tomorrow !! wow', 'Hello!!1 !!9 p1', 'https://x/y #unknown', '"!!1"']) assert.equal(parseQuickAdd(name).name, name);
});
test('quoted priorities, escaped tokens and unknown syntax remain literal', async () => {
 const { parseQuickAdd, activeShortcut, shortcutOptions } = await helpers();
 for (const name of ['Say "use !!1 here" today', 'Buy \\#Work', 'Use \\!!1', 'literal !!0 !!5 !!!1', '  spaced   task  ']) assert.equal(parseQuickAdd(name).name, name.trim());
 assert.equal(activeShortcut('Use \\#Work', 10), null);
 assert.equal(shortcutOptions(activeShortcut('/nonsense', 9), projects, '2026-07-06').length, 0);
 assert.equal(activeShortcut('Buy #Homework', 8), null);
});
test('cursor-aware project queries support spaces and quotes and preserve suffix', async () => {
 const { activeShortcut, shortcutOptions, consumeShortcut } = await helpers();
 const text = 'Buy #Home Office later';
 const token = activeShortcut(text, 16);
 assert.equal(token.query, 'Home Office');
 assert.equal(shortcutOptions(token, projects)[0].project_id, 'a');
 assert.deepEqual(consumeShortcut(text, token), { name: 'Buy later', cursor: 4 });
 assert.equal(activeShortcut('C# code', 7), null);
 assert.equal(activeShortcut('Buy #"Home Office"', 18).query, 'Home Office');
 assert.equal(shortcutOptions(activeShortcut('#missing', 8), projects).length, 0);
});
test('date/repeat shortcuts use supported values only on confirmation', async () => {
 const { activeShortcut, shortcutOptions, parseQuickAdd } = await helpers();
 assert.equal(shortcutOptions(activeShortcut('/tomorrow', 9), projects, '2026-07-06')[0].due_date, '2026-07-07');
 const repeat = shortcutOptions(activeShortcut('/every week', 11), projects, '2026-07-06')[0];
 assert.equal(repeat.repeat_rule, 'weekly');
 assert.equal(repeat.due_date, '2026-07-06');
 assert.equal(parseQuickAdd('Call tomorrow /tomorrow #Work').name, 'Call tomorrow /tomorrow #Work');
});
test('numeric date shortcuts keep their internal slashes', async () => {
 const { activeShortcut, shortcutOptions } = await helpers();
 for (const [text, expected] of [['Task /7/20', '2026-07-20'], ['Task /7/20/2027', '2027-07-20']]) {
   const token = activeShortcut(text, text.length);
   assert.equal(token.kind, '/');
   assert.equal(shortcutOptions(token, projects, '2026-07-06')[0].due_date, expected);
 }
});
test('Shift+Tab leaves suggestions without confirming them', () => {
 const source = readFileSync(new URL('./components/QuickAdd.svelte', import.meta.url), 'utf8');
 const body = source.match(/function inputKeydown\(e\) \{([\s\S]*?)\n  \}/)[1];
 let selected = false;
 const handler = new Function('e', 'token', 'suggestions', 'choose', 'activeIndex', body);
 handler({ key: 'Tab', shiftKey: true, preventDefault() { throw new Error('Shift+Tab must not be trapped'); } }, {}, [{}], () => { selected = true; }, 0);
 assert.equal(selected, false);
});
test('all supported priorities and recurrence choices are suggested with exact API values', async () => {
 const { activeShortcut, shortcutOptions, parseQuickAdd } = await helpers();
 assert.deepEqual(shortcutOptions(activeShortcut('!!', 2), projects).map(o => o.priority), [1, 2, 3, 4]);
 for (const priority of [1, 2, 3, 4]) assert.deepEqual(parseQuickAdd(`Task !!${priority}`), { name: 'Task', priority });
 assert.deepEqual(shortcutOptions(activeShortcut('/every', 6), projects, '2026-07-06').map(o => o.repeat_rule), ['daily', 'weekly', 'monthly', 'yearly']);
 assert.equal(shortcutOptions(activeShortcut('/no date', 8), projects, '2026-07-06')[0].due_date, null);
 assert.equal(shortcutOptions(activeShortcut('/in 3 days', 10), projects, '2026-07-06')[0].due_date, '2026-07-09');
});
test('project filtering is case-insensitive and consumption preserves text before and after the cursor', async () => {
 const { activeShortcut, shortcutOptions, consumeShortcut } = await helpers();
 assert.equal(shortcutOptions(activeShortcut('#OFFICE', 7), projects)[0].project_id, 'a');
 const text = 'Do #"Home Office" later';
 const token = activeShortcut(text, 17);
 assert.equal(token.query, 'Home Office');
 assert.deepEqual(consumeShortcut(text, token), { name: 'Do later', cursor: 3 });
 assert.deepEqual(consumeShortcut('#Work', activeShortcut('#Work', 5)), { name: '', cursor: 0 });
});
test('suggestion pointer-down preserves mouse caret but never consumes touch scrolling', () => {
 const source = readFileSync(new URL('./components/QuickAdd.svelte', import.meta.url), 'utf8');
 const body = source.match(/function keepInputFocus\(e\) \{([\s\S]*?)\n  \}/)[1];
 const handler = new Function('e', body);
 for (const pointerType of ['mouse', 'touch', 'pen']) {
   let prevented = false;
   handler({ pointerType, preventDefault() { prevented = true; } });
   assert.equal(prevented, pointerType === 'mouse');
 }
 assert.ok(source.includes('onpointerdown={keepInputFocus} onclick={() => choose(option)}'));
 assert.ok(!source.includes('onblur={() => (dismissed = true)}'));
});
test('composer exposes themed metadata, selected priority and compact accessible add action', () => {
 const source = readFileSync(new URL('./components/QuickAdd.svelte', import.meta.url), 'utf8');
 for (const marker of ['selectedProject?.color', 'data-priority={effectivePriority}', 'aria-label="Add task"', 'suggestion-icon', ':global(.dark) .suggestion', 'id="quick-add-hint"', 'min-height:2.75rem']) assert.ok(source.includes(marker), marker);
 for (const id of ['description', 'repeat', 'deadline', 'priority', 'project']) assert.ok(source.includes(id), id);
});
test('compact UI wires accessible suggestions keyboard selection focus preservation optional details', () => {
 const source = readFileSync(new URL('./components/QuickAdd.svelte', import.meta.url), 'utf8');
 for (const marker of ['role="combobox"', 'role="listbox"', 'aria-activedescendant', 'onpointerdown', 'ArrowDown', 'ArrowUp', "'Tab'", 'parseQuickAdd(name)', '<details', 'Description', 'normalizeRepeatRule']) assert.ok(source.includes(marker), marker);
});
