<script>
  import { data, navigate, addProject, reorderProjects, shareProjects, toast } from '../state.svelte.js';
  import { bulkShareMessage } from '../bulkShare.js';
  import { Hash, Plus, Users, ChevronRight, GripVertical, UserPlus, X } from '@lucide/svelte';

  const INDENT = 24; // px per nesting level; dragging this far sideways changes depth

  // Depth-first flattening of the project tree. Projects whose parent isn't
  // visible (e.g. a shared child of an unshared parent) render as roots.
  const rows = $derived.by(() => {
    const list = data.projects.filter((p) => !p.is_inbox);
    const ids = new Set(list.map((p) => p.id));
    const out = [];
    const add = (parentId, depth) => {
      for (const p of list) {
        const pid = p.parent_id && ids.has(p.parent_id) ? p.parent_id : null;
        if (pid === parentId) {
          out.push({ p, depth });
          add(p.id, depth + 1);
        }
      }
    };
    add(null, 0);
    return out;
  });

  let adding = $state(false);
  let newName = $state('');
  let listEl = $state(null);
  let drag = $state(null); // { id, subtree:Set, origDepth, startX, indicator }
  let bulkShareOpen = $state(false);
  let bulkEmail = $state('');
  let bulkBusy = $state(false);
  let selectedProjectIds = $state(new Set());

  const selectedCount = $derived(selectedProjectIds.size);

  function selectableProjectIds() {
    return rows.map((r) => r.p.id);
  }

  function openBulkShare() {
    selectedProjectIds = new Set(selectableProjectIds());
    bulkShareOpen = true;
  }

  function closeBulkShare() {
    if (bulkBusy) return;
    bulkShareOpen = false;
  }

  function toggleBulkProject(id) {
    const next = new Set(selectedProjectIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedProjectIds = next;
  }

  function setAllBulkProjects(checked) {
    selectedProjectIds = checked ? new Set(selectableProjectIds()) : new Set();
  }

  async function submitBulkShare(e) {
    e.preventDefault();
    if (!bulkEmail.trim() || selectedProjectIds.size === 0 || bulkBusy) return;
    bulkBusy = true;
    try {
      const email = bulkEmail.trim();
      const result = await shareProjects([...selectedProjectIds], email);
      toast(bulkShareMessage({ email, ...result }));
      bulkEmail = '';
      selectedProjectIds = new Set();
      bulkShareOpen = false;
    } catch (err) {
      toast(err.message);
    } finally {
      bulkBusy = false;
    }
  }

  async function createProject(e) {
    e.preventDefault();
    if (!newName.trim()) return;
    try {
      const p = await addProject({ name: newName });
      newName = '';
      adding = false;
      navigate('project', p.id);
    } catch (err) {
      toast(err.message);
    }
  }

  function subtreeIds(id) {
    const s = new Set([id]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const r of rows) {
        if (!s.has(r.p.id) && r.p.parent_id && s.has(r.p.parent_id)) {
          s.add(r.p.id);
          grew = true;
        }
      }
    }
    return s;
  }

  function startDrag(e, row) {
    e.preventDefault();
    drag = {
      id: row.p.id,
      subtree: subtreeIds(row.p.id),
      origDepth: row.depth,
      startX: e.clientX,
      indicator: null
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', endDrag, { once: true });
    onMove(e);
  }

  function candidateEls() {
    return [...listEl.querySelectorAll('[data-pid]')].filter(
      (el) => !drag.subtree.has(Number(el.dataset.pid))
    );
  }

  function onMove(e) {
    if (!drag) return;
    const els = candidateEls();
    const listTop = listEl.getBoundingClientRect().top;
    let index = 0;
    for (const el of els) {
      const r = el.getBoundingClientRect();
      if (e.clientY > r.top + r.height / 2) index++;
      else break;
    }
    const vis = rows.filter((r) => !drag.subtree.has(r.p.id));
    const above = vis[index - 1];
    const below = vis[index];
    const maxDepth = above ? above.depth + 1 : 0;
    const minDepth = below ? Math.min(below.depth, maxDepth) : 0;
    let depth = drag.origDepth + Math.round((e.clientX - drag.startX) / INDENT);
    depth = Math.max(minDepth, Math.min(maxDepth, depth));
    const y =
      index === 0
        ? els.length
          ? els[0].getBoundingClientRect().top - listTop
          : 0
        : els[index - 1].getBoundingClientRect().bottom - listTop;
    drag.indicator = { index, depth, y };
  }

  async function endDrag() {
    window.removeEventListener('pointermove', onMove);
    const d = drag;
    drag = null;
    if (!d?.indicator) return;

    const vis = rows.filter((r) => !d.subtree.has(r.p.id));
    const moved = rows.filter((r) => d.subtree.has(r.p.id));
    const delta = d.indicator.depth - d.origDepth;
    const newRows = [
      ...vis.slice(0, d.indicator.index),
      ...moved.map((r) => ({ p: r.p, depth: r.depth + delta })),
      ...vis.slice(d.indicator.index)
    ];
    const unchanged =
      newRows.length === rows.length &&
      newRows.every((r, i) => r.p.id === rows[i].p.id && r.depth === rows[i].depth);
    if (unchanged) return;

    // Depths → parents: the parent of a row is the last row seen one level up.
    const stack = [];
    const items = newRows.map((r, i) => {
      const parent_id = r.depth > 0 ? stack[r.depth - 1] : null;
      stack[r.depth] = r.p.id;
      stack.length = r.depth + 1;
      return { id: r.p.id, parent_id, sort_order: i + 1 };
    });
    await reorderProjects(items);
  }
</script>

<header class="mb-4 flex items-center justify-between">
  <h1 class="text-2xl font-bold tracking-tight">Projects</h1>
  <div class="flex items-center gap-1">
    {#if rows.length}
      <button
        aria-label="Share multiple projects"
        onclick={openBulkShare}
        class="rounded p-1.5 text-zinc-400 hover:text-brand-600"
      >
        <UserPlus size={19} />
      </button>
    {/if}
    <button
      aria-label="Add project"
      onclick={() => (adding = !adding)}
      class="rounded p-1.5 text-zinc-400 hover:text-brand-600"
    >
      <Plus size={20} />
    </button>
  </div>
</header>

{#if adding}
  <form onsubmit={createProject} class="mb-2">
    <input
      bind:value={newName}
      placeholder="Project name"
      class="w-full rounded-lg border border-zinc-300 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
    />
  </form>
{/if}

{#if bulkShareOpen}
  <div
    class="fixed inset-0 z-40 flex items-end justify-center bg-black/40 sm:items-center sm:p-6"
    role="presentation"
    onclick={(e) => e.target === e.currentTarget && closeBulkShare()}
  >
    <form
      onsubmit={submitBulkShare}
      class="w-full rounded-t-2xl bg-white p-5 shadow-xl sm:max-w-md sm:rounded-2xl dark:bg-zinc-900"
    >
      <div class="flex items-center justify-between">
        <div>
          <h2 class="text-lg font-semibold">Share projects</h2>
          <p class="text-xs text-zinc-400">Choose projects and invite someone by email.</p>
        </div>
        <button type="button" aria-label="Close" onclick={closeBulkShare} class="p-1 text-zinc-400 hover:text-zinc-600">
          <X size={18} />
        </button>
      </div>

      <label class="mt-4 block">
        <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Email</span>
        <input
          bind:value={bulkEmail}
          type="email"
          placeholder="friend@example.com"
          class="mt-1 w-full rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
        />
      </label>

      <div class="mt-4 flex items-center justify-between">
        <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Projects</span>
        <label class="flex items-center gap-2 text-xs text-zinc-500">
          <input
            type="checkbox"
            checked={selectedCount === rows.length}
            onchange={(e) => setAllBulkProjects(e.target.checked)}
          />
          Select all
        </label>
      </div>

      <div class="mt-2 max-h-72 space-y-1 overflow-y-auto rounded-xl border border-zinc-200 p-2 dark:border-zinc-800">
        {#each rows as row (row.p.id)}
          <label
            class="flex items-center gap-3 rounded-lg px-2 py-2 text-sm hover:bg-zinc-100 dark:hover:bg-zinc-800"
            style="padding-left: {0.5 + row.depth * 1.25}rem"
          >
            <input
              type="checkbox"
              checked={selectedProjectIds.has(row.p.id)}
              onchange={() => toggleBulkProject(row.p.id)}
            />
            <Hash size={15} class="flex-none text-zinc-400" />
            <span class="min-w-0 flex-1 truncate">{row.p.name}</span>
            {#if row.p.members?.length > 1}
              <Users size={13} class="flex-none text-zinc-400" />
            {/if}
          </label>
        {/each}
      </div>

      <p class="mt-2 text-xs text-zinc-400">
        {selectedCount} {selectedCount === 1 ? 'project' : 'projects'} selected. Shared projects and tasks sync live.
      </p>

      <button
        type="submit"
        disabled={!bulkEmail.trim() || selectedCount === 0 || bulkBusy}
        class="mt-4 flex w-full items-center justify-center gap-2 rounded-lg bg-brand-600 px-3 py-2.5 text-sm font-semibold text-white disabled:opacity-40"
      >
        <UserPlus size={16} /> {bulkBusy ? 'Sharing…' : `Share ${selectedCount || ''} projects`}
      </button>
    </form>
  </div>
{/if}

<div bind:this={listEl} class="relative mt-1">
  {#each rows as row (row.p.id)}
    <div
      data-pid={row.p.id}
      class="flex w-full items-center gap-1 border-b border-zinc-100 dark:border-zinc-800/70 {drag?.subtree.has(
        row.p.id
      )
        ? 'opacity-40'
        : ''}"
      style="padding-left: {row.depth * INDENT}px"
    >
      <button
        aria-label="Drag to reorder or nest"
        onpointerdown={(e) => startDrag(e, row)}
        class="-ml-1.5 cursor-grab touch-none p-1.5 text-zinc-300 hover:text-zinc-500 active:cursor-grabbing dark:text-zinc-600 dark:hover:text-zinc-400"
      >
        <GripVertical size={15} />
      </button>
      <button
        onclick={() => navigate('project', row.p.id)}
        class="flex min-w-0 flex-1 items-center gap-3 py-3 text-left"
      >
        <Hash size={17} class="flex-none text-zinc-400" />
        <span class="min-w-0 flex-1 truncate text-sm font-medium">{row.p.name}</span>
        {#if row.p.members?.length > 1}
          <Users size={14} class="flex-none text-zinc-400" />
        {/if}
        {#if row.p.active_count}
          <span class="text-xs text-zinc-400">{row.p.active_count}</span>
        {/if}
        <ChevronRight size={16} class="text-zinc-300 dark:text-zinc-600" />
      </button>
    </div>
  {:else}
    {#if !adding}
      <p class="py-3 text-sm text-zinc-400">No projects yet — create one to organize your tasks.</p>
    {/if}
  {/each}
  {#if drag?.indicator}
    <div
      class="pointer-events-none absolute right-0 z-10 h-0.5 rounded bg-brand-500"
      style="top: {drag.indicator.y - 1}px; left: {drag.indicator.depth * INDENT}px"
    ></div>
  {/if}
</div>

{#if rows.length > 1}
  <p class="mt-3 text-xs text-zinc-400">
    Drag the handle to reorder — drag right while dropping to nest inside the project above.
  </p>
{/if}
