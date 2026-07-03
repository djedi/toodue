<script>
  import { data, ui } from '../state.svelte.js';
  import { todayStr, fullDayLabel, dayLabel } from '../dates.js';
  import TaskList from '../components/TaskList.svelte';
  import { Plus } from '@lucide/svelte';

  const today = $derived(todayStr());
  const upcoming = $derived(
    data.tasks.filter((t) => t.due_date && t.due_date > today && !t.completed_at)
  );
  const days = $derived(
    [...new Set(upcoming.map((t) => t.due_date))].sort()
  );

  function tasksOn(day) {
    return upcoming
      .filter((t) => t.due_date === day)
      .sort(
        (a, b) => (a.due_time ?? '99').localeCompare(b.due_time ?? '99') || a.priority - b.priority
      );
  }
</script>

<header class="mb-4">
  <h1 class="text-2xl font-bold tracking-tight">Upcoming</h1>
</header>

{#each days as day (day)}
  <section class="mb-6">
    <div class="flex items-baseline justify-between border-b border-zinc-200 pb-1.5 dark:border-zinc-800">
      <h2 class="text-sm font-semibold">{fullDayLabel(day)}</h2>
      <button
        aria-label="Add task on {dayLabel(day)}"
        onclick={() => (ui.quickAdd = { due_date: day })}
        class="p-0.5 text-zinc-400 hover:text-brand-600"
      >
        <Plus size={16} />
      </button>
    </div>
    <TaskList tasks={tasksOn(day)} showProject />
  </section>
{:else}
  <div class="mt-16 text-center">
    <p class="font-medium">Nothing scheduled</p>
    <p class="mt-1 text-sm text-zinc-400">Tasks with future dates will show up here.</p>
    <button
      onclick={() => (ui.quickAdd = {})}
      class="mx-auto mt-4 flex items-center gap-2 text-sm text-brand-600 hover:underline"
    >
      <Plus size={16} /> Add a task
    </button>
  </div>
{/each}
