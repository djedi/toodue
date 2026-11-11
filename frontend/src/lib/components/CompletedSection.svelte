<script>
  import { api } from '../api.js';
  import { bus, toast, isTaskDeleted, refreshTasks, data, mutationSession, sessionValid, markTasksDeleted } from '../state.svelte.js';
  import { completedQuery, matchesCompletedFilter, purgeMessage } from '../completed.js';
  import TaskItem from './TaskItem.svelte';
  import { Trash2 } from '@lucide/svelte';

  // `filter` describes the view's completed set (see completed.js). `scope` names it in the
  // purge confirmation, e.g. "in #Yard" or "completed today".
  let { filter = {}, scope = '', showProject = false } = $props();

  let show = $state(false);
  let tasks = $state([]);
  // The server caps the list at 200, so the last load may not have returned every match.
  let truncated = $state(false);
  let purging = $state(false);
  let loading = false;
  let gen = 0; // bumped when the filter changes so stale responses are ignored
  let loadSeq = 0; // each load gets its own number; only the latest one applies
  // Rows removed optimistically, kept so a failed delete can put them back.
  let removed = new Map();

  const key = $derived(completedQuery(filter));

  $effect(() => {
    // A different filter (e.g. another project) means a different list.
    key;
    show = false;
    tasks = [];
    truncated = false;
    removed = new Map();
    gen++;
  });

  // A mutation landed while the list was loading: that response is now stale, fetch again.
  function invalidateLoad() {
    if (!loading) return;
    gen++;
    load();
  }

  $effect(() => {
    const onDeleted = (e) => {
      invalidateLoad();
      const id = e.detail.id;
      const gone = tasks.filter((t) => t.id === id || t.parent_id === id);
      if (gone.length) removed.set(id, gone);
      tasks = tasks.filter((t) => t.id !== id && t.parent_id !== id);
    };
    const onDeleteFailed = (e) => {
      const id = e.detail.id;
      const gone = removed.get(id);
      removed.delete(id);
      if (!gone) return;
      const present = new Set(tasks.map((t) => t.id));
      tasks = [...tasks, ...gone.filter((t) => !present.has(t.id))];
    };
    // Confirmed edits, local or from another client: tasks that leave the set drop out, ones
    // that enter it appear on top.
    const applyServerTask = (t) => {
      invalidateLoad();
      if (isTaskDeleted(t.id) || isTaskDeleted(t.parent_id)) return;
      // A project we no longer have (deleted, or access revoked) shows nothing, whatever a late
      // response says.
      if (!data.projects.some((p) => p.id === t.project_id)) return;
      const belongs = matchesCompletedFilter(t, filter);
      const present = tasks.some((x) => x.id === t.id);
      if (!belongs) {
        // A parent moved to another project takes its subtasks along on the server; a reopened
        // parent leaves its completed subtasks alone.
        const movedOut = filter.project_id != null && t.project_id !== Number(filter.project_id);
        tasks = tasks.filter((x) => x.id !== t.id && !(movedOut && x.parent_id === t.id));
      } else if (present) {
        tasks = tasks.map((x) => (x.id === t.id ? t : x));
      } else if (show) {
        tasks = [t, ...tasks];
      }
    };
    const onUpdated = (e) => applyServerTask(e.detail);
    const onServerEvent = (e) => {
      const { type, data: d } = e.detail;
      if (type === 'task.upsert') applyServerTask(d);
      else if (type === 'project.remove') {
        // The project was deleted or we lost access: none of its tasks may stay visible.
        invalidateLoad();
        tasks = tasks.filter((t) => t.project_id !== d.id);
        for (const [k, held] of removed) {
          const kept = held.filter((t) => t.project_id !== d.id);
          if (kept.length) removed.set(k, kept);
          else removed.delete(k);
        }
      }
    };
    // Deleted outside the local pipeline (another client, a purge): nothing to roll back.
    const onRemoved = (e) => {
      invalidateLoad();
      const gone = new Set(e.detail.ids);
      const isGone = (t) => gone.has(t.id) || gone.has(t.parent_id);
      tasks = tasks.filter((t) => !isGone(t));
      // A local delete of the same rows that fails later must not restore them either.
      for (const [k, held] of removed) {
        const kept = gone.has(k) ? [] : held.filter((t) => !isGone(t));
        if (kept.length) removed.set(k, kept);
        else removed.delete(k);
      }
    };
    // Another client purged or imported: the list we show may be gone.
    const onRefreshed = () => {
      if (show) {
        gen++;
        load();
      }
    };
    bus.addEventListener('task-deleted', onDeleted);
    bus.addEventListener('task-delete-failed', onDeleteFailed);
    bus.addEventListener('task-updated', onUpdated);
    bus.addEventListener('server-event', onServerEvent);
    bus.addEventListener('tasks-removed', onRemoved);
    bus.addEventListener('tasks-refreshed', onRefreshed);
    return () => {
      bus.removeEventListener('task-deleted', onDeleted);
      bus.removeEventListener('task-delete-failed', onDeleteFailed);
      bus.removeEventListener('task-updated', onUpdated);
      bus.removeEventListener('server-event', onServerEvent);
      bus.removeEventListener('tasks-removed', onRemoved);
      bus.removeEventListener('tasks-refreshed', onRefreshed);
    };
  });

  async function load() {
    const myGen = gen;
    const mySeq = ++loadSeq;
    const current = () => myGen === gen && mySeq === loadSeq;
    const query = key;
    loading = true;
    try {
      const rows = await api.get(`/tasks?${query}`);
      if (!current()) return;
      // Rows deleted while this request was in flight are held back, but kept for rollback.
      for (const t of rows) {
        const k = isTaskDeleted(t.id) ? t.id : isTaskDeleted(t.parent_id) ? t.parent_id : null;
        if (k === null) continue;
        const held = removed.get(k) ?? [];
        if (!held.some((h) => h.id === t.id)) removed.set(k, [...held, t]);
      }
      truncated = rows.length >= 200;
      tasks = rows.filter((t) => !isTaskDeleted(t.id) && !isTaskDeleted(t.parent_id));
    } catch (err) {
      if (current()) toast(err.message);
    } finally {
      if (current()) loading = false;
    }
  }

  async function toggle() {
    show = !show;
    if (show) await load();
  }

  async function purge() {
    if (purging || !tasks.length) return;
    const where = scope ? ` ${scope}` : '';
    // The list is capped by the server, so say so when there may be more than are shown.
    const what = `all completed tasks${where}` + (truncated ? ', including ones not shown here' : ` (${tasks.length} shown)`);
    if (!confirm(`Permanently delete ${what}? Their subtasks go too. This cannot be undone.`)) return;
    purging = true;
    const session = mutationSession();
    const myGen = gen;
    try {
      const r = await api.del(`/tasks/completed?${key}`);
      if (!sessionValid(session)) return;
      // Tombstone them now so a late edit response can't put one back.
      markTasksDeleted(r.ids ?? []);
      if (myGen === gen) {
        const gone = new Set(r.ids ?? []);
        tasks = tasks.filter((t) => !gone.has(t.id));
      }
      toast(purgeMessage(r));
      // Parents of purged subtasks show new counts.
      await refreshTasks().catch(() => {});
    } catch (err) {
      if (sessionValid(session)) toast(err.message);
    } finally {
      purging = false;
    }
  }
</script>

<div class="mt-8 border-t border-zinc-100 pt-3 dark:border-zinc-800">
  <div class="flex items-center justify-between gap-2">
    <button onclick={toggle} class="text-xs font-medium text-zinc-400 hover:text-zinc-600">
      {show ? 'Hide completed' : 'Show completed'}
    </button>
    {#if show && tasks.length}
      <button
        onclick={purge}
        disabled={purging || data.offline}
        class="flex items-center gap-1 text-xs font-medium text-zinc-400 hover:text-red-600 disabled:opacity-50 dark:hover:text-red-400"
      >
        <Trash2 size={13} />
        {purging ? 'Deleting…' : 'Delete completed'}
      </button>
    {/if}
  </div>
  {#if show}
    {#if tasks.length}
      <ul class="mt-1 opacity-70">
        {#each tasks as task (task.id)}
          <TaskItem {task} {showProject} />
        {/each}
      </ul>
    {:else}
      <p class="mt-2 text-xs text-zinc-400">No completed tasks here.</p>
    {/if}
  {/if}
</div>
