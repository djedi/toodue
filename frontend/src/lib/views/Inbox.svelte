<script>
  import { data, ui, inboxProject } from '../state.svelte.js';
  import TaskList from '../components/TaskList.svelte';
  import { Plus, Inbox } from '@lucide/svelte';

  const inbox = $derived(inboxProject());
  const tasks = $derived(
    data.tasks
      .filter((t) => t.project_id === inbox?.id && !t.completed_at)
      .sort((a, b) => a.priority - b.priority || a.sort_order - b.sort_order)
  );
</script>

<header class="mb-4">
  <h1 class="text-2xl font-bold tracking-tight">Inbox</h1>
</header>

{#if tasks.length}
  <TaskList {tasks} nest />
{:else}
  <div class="mt-16 flex flex-col items-center gap-3 text-center">
    <Inbox size={40} class="text-zinc-300 dark:text-zinc-600" />
    <p class="font-medium">Your inbox is empty</p>
    <p class="text-sm text-zinc-400">Capture anything on your mind — sort it later.</p>
  </div>
{/if}

<button
  onclick={() => (ui.quickAdd = { project_id: inbox?.id })}
  class="mt-3 flex items-center gap-2 text-sm text-zinc-500 transition hover:text-brand-600"
>
  <span class="flex h-5 w-5 items-center justify-center rounded-full text-brand-600"><Plus size={16} /></span>
  Add task
</button>
