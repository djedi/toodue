<script>
  import { data, ui, updateTask, deleteTask, endSelection, resolveId, mutationSession, sessionValid, toast } from '../state.svelte.js';
  import DatePicker from './DatePicker.svelte';
  import { mapLimit } from '../bulk.js';
  import { X, CalendarDays, Hash, Flag, Trash2 } from '@lucide/svelte';

  let picking = $state(false);
  let busy = $state(false);
  const count = $derived(ui.selectedIds.length);

  // Only the selection that started an operation is affected by it, and only the ids it processed
  // leave the selection; anything selected while it ran stays selected. `alsoDone` maps an id to
  // other selected ids that are covered by its success (e.g. subtasks of a moved parent).
  async function runBulk(ids, verb, fn, alsoDone = new Map()) {
    const gen = ui.selectionGen;
    const session = mutationSession();
    busy = true;
    const results = await mapLimit(ids, 4, (id) => {
      if (!sessionValid(session)) throw new Error('Signed out');
      return fn(id);
    });
    busy = false;
    if (!sessionValid(session)) return;
    const failed = results.filter((r) => r.status === 'rejected');
    if (failed.length) toast(`${failed.length} of ${ids.length} tasks could not be ${verb}: ${failed[0].reason?.message ?? ''}`);
    if (!ui.selecting || ui.selectionGen !== gen) return;
    // Successful ids leave the selection (failed ones stay so the user can retry). A task created
    // mid-operation was remapped in the selection to its real id; match on that too.
    const okIds = ids.filter((_, i) => results[i].status === 'fulfilled');
    const covered = okIds.flatMap((id) => alsoDone.get(id) ?? []);
    const done = new Set([...okIds, ...covered, ...(await Promise.all([...okIds, ...covered].map(resolveId)))]);
    if (!ui.selecting || ui.selectionGen !== gen) return;
    const rest = ui.selectedIds.filter((id) => !done.has(id));
    if (failed.length || rest.length) {
      ui.selectedIds = rest;
      for (const id of done) delete ui.selectedParents[id];
    } else {
      endSelection();
    }
  }

  function applyAll(fields) {
    if (!count || busy) return;
    const ids = [...ui.selectedIds];
    if ('project_id' in fields) {
      // Subtasks live in their parent's project; moving one on its own would make it vanish.
      const parents = ids.filter((id) => ui.selectedParents[id] == null);
      if (parents.length < ids.length) toast(`Subtasks move with their parent${parents.length ? '' : ' — select the parent task'}.`);
      if (!parents.length) return;
      // Selected children of a moving parent are moved by the server along with it.
      const children = new Map(parents.map((pid) => [pid, ids.filter((id) => ui.selectedParents[id] === pid)]));
      return runBulk(parents, 'updated', (id) => updateTask(id, fields), children);
    }
    return runBulk(ids, 'updated', (id) => updateTask(id, fields));
  }

  function removeAll() {
    if (!count || busy) return;
    if (!confirm(`Delete ${count} task${count === 1 ? '' : 's'}?`)) return;
    // Deleting a parent cascades to its subtasks; don't also delete those separately.
    const selected = new Set(ui.selectedIds);
    const ids = ui.selectedIds.filter((id) => {
      const parent = ui.selectedParents[id];
      return !(parent != null && selected.has(parent));
    });
    return runBulk(ids, 'deleted', (id) => deleteTask(id));
  }

  function onProject(e) {
    const v = e.target.value;
    e.target.value = '';
    if (v) applyAll({ project_id: Number(v) });
  }

  function onPriority(e) {
    const v = e.target.value;
    e.target.value = '';
    if (v) applyAll({ priority: Number(v) });
  }

  function onkeydown(e) {
    if (e.key === 'Escape' && !picking) endSelection();
  }

  const action =
    'relative flex flex-col items-center gap-0.5 rounded-lg px-2 py-1.5 text-[11px] font-medium text-zinc-600 transition active:bg-zinc-100 disabled:opacity-40 dark:text-zinc-300 dark:active:bg-zinc-800';
  // Invisible native <select> stretched over the button so it opens the system picker.
  const overlay = 'absolute inset-0 cursor-pointer opacity-0';
</script>

<svelte:window {onkeydown} />

<div
  role="toolbar"
  aria-label="Selected tasks"
  class="fixed inset-x-0 bottom-0 z-30 border-t border-zinc-200 bg-white/95 backdrop-blur md:inset-x-auto md:right-8 md:bottom-6 md:left-1/2 md:w-auto md:-translate-x-1/2 md:rounded-2xl md:border md:shadow-xl dark:border-zinc-800 dark:bg-zinc-900/95"
  style="padding-bottom: env(safe-area-inset-bottom)"
>
  <div class="flex items-center gap-1 px-2 py-1.5">
    <button aria-label="Cancel selection" onclick={endSelection} class="rounded-lg p-2 text-zinc-500 active:bg-zinc-100 dark:active:bg-zinc-800">
      <X size={18} />
    </button>
    <span class="min-w-[5.5rem] text-sm font-semibold">{count} selected</span>

    <div class="ml-auto flex items-center">
      <button class={action} disabled={!count || busy} onclick={() => (picking = true)}>
        <CalendarDays size={18} />
        Date
      </button>
      <div class={action} class:opacity-40={!count || busy}>
        <Hash size={18} />
        Project
        <select aria-label="Move to project" class={overlay} disabled={!count || busy} onchange={onProject}>
          <option value=""></option>
          {#each data.projects as p (p.id)}
            <option value={p.id}>{p.is_inbox ? 'Inbox' : p.name}</option>
          {/each}
        </select>
      </div>
      <div class={action} class:opacity-40={!count || busy}>
        <Flag size={18} />
        Priority
        <select aria-label="Set priority" class={overlay} disabled={!count || busy} onchange={onPriority}>
          <option value=""></option>
          <option value="1">P1 · Urgent</option>
          <option value="2">P2 · High</option>
          <option value="3">P3 · Medium</option>
          <option value="4">P4 · Normal</option>
        </select>
      </div>
      <button class="{action} text-red-600 dark:text-red-400" disabled={!count || busy} onclick={removeAll}>
        <Trash2 size={18} />
        Delete
      </button>
    </div>
  </div>
</div>

{#if picking}
  <DatePicker
    title="Due date"
    allowsTime={false}
    onselect={(due_date) => applyAll(due_date ? { due_date } : { due_date: null, due_time: null })}
    onclose={() => (picking = false)}
  />
{/if}
