<script>
  import { data, ui, addTask, inboxProject, toast } from '../state.svelte.js';
  import { todayStr } from '../dates.js';
  import { CalendarDays, Flag, X } from '@lucide/svelte';

  let name = $state('');
  let description = $state('');
  let due_date = $state(ui.quickAdd?.due_date ?? '');
  let due_time = $state('');
  let deadline = $state('');
  let priority = $state(4);
  let project_id = $state(
    ui.quickAdd?.project_id ??
      (ui.view === 'project' ? ui.projectId : null) ??
      inboxProject()?.id
  );
  let busy = $state(false);
  let nameInput = $state(null);

  $effect(() => {
    nameInput?.focus();
  });

  const selectable = $derived(data.projects);

  function close() {
    ui.quickAdd = null;
  }

  async function submit(e) {
    e?.preventDefault();
    if (!name.trim() || busy) return;
    busy = true;
    try {
      await addTask({
        project_id,
        name,
        description,
        due_date: due_date || null,
        due_time: due_date && due_time ? due_time : null,
        deadline: deadline || null,
        priority: Number(priority)
      });
      close();
    } catch (err) {
      toast(err.message);
    } finally {
      busy = false;
    }
  }

  function onkeydown(e) {
    if (e.key === 'Escape') close();
  }

  const chip =
    'flex items-center gap-1.5 rounded-lg border border-zinc-200 px-2 py-1.5 text-xs text-zinc-600 dark:border-zinc-700 dark:text-zinc-300';
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-40 flex items-end justify-center bg-black/40 sm:items-start sm:pt-[18vh]"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && close()}
>
  <form
    onsubmit={submit}
    class="w-full rounded-t-2xl bg-white p-4 shadow-xl sm:max-w-xl sm:rounded-2xl dark:bg-zinc-900"
    style="padding-bottom: max(1rem, env(safe-area-inset-bottom))"
  >
    <div class="flex items-start justify-between">
      <input
        bind:this={nameInput}
        bind:value={name}
        placeholder="Task name"
        class="w-full bg-transparent text-lg font-medium outline-none placeholder:text-zinc-400"
      />
      <button type="button" aria-label="Close" onclick={close} class="p-1 text-zinc-400 hover:text-zinc-600">
        <X size={18} />
      </button>
    </div>
    <textarea
      bind:value={description}
      placeholder="Description"
      rows="2"
      class="mt-1 w-full resize-none bg-transparent text-sm outline-none placeholder:text-zinc-400"
    ></textarea>

    <div class="mt-2 flex flex-wrap items-center gap-2">
      <label class={chip}>
        <CalendarDays size={14} class="text-emerald-600" />
        <input type="date" bind:value={due_date} class="bg-transparent outline-none" />
        {#if !due_date}
          <button type="button" class="text-emerald-600" onclick={() => (due_date = todayStr())}>
            Today
          </button>
        {/if}
      </label>
      {#if due_date}
        <label class={chip}>
          <input type="time" bind:value={due_time} class="bg-transparent outline-none" />
        </label>
      {/if}
      <label class={chip} title="Deadline">
        <Flag size={14} class="text-amber-600" />
        <input type="date" bind:value={deadline} class="bg-transparent outline-none" />
      </label>
      <select
        bind:value={priority}
        class="{chip} bg-transparent outline-none"
        title="Priority"
      >
        <option value={1}>Priority 1</option>
        <option value={2}>Priority 2</option>
        <option value={3}>Priority 3</option>
        <option value={4}>Priority 4</option>
      </select>
    </div>

    <div class="mt-3 flex items-center justify-between border-t border-zinc-100 pt-3 dark:border-zinc-800">
      <select
        bind:value={project_id}
        class="max-w-[55%] rounded-lg border border-zinc-200 bg-transparent px-2 py-1.5 text-xs text-zinc-600 outline-none dark:border-zinc-700 dark:text-zinc-300"
      >
        {#each selectable as p (p.id)}
          <option value={p.id}>{p.is_inbox ? 'Inbox' : p.name}</option>
        {/each}
      </select>
      <button
        type="submit"
        disabled={!name.trim() || busy}
        class="rounded-lg bg-brand-600 px-4 py-2 text-sm font-semibold text-white transition hover:bg-brand-700 disabled:opacity-40"
      >
        Add task
      </button>
    </div>
  </form>
</div>
