<script>
  import { data, ui, updateTask, toast } from '../state.svelte.js';
  import { todayStr, fullDayLabel } from '../dates.js';
  import TaskList from '../components/TaskList.svelte';
  import DatePicker from '../components/DatePicker.svelte';
  import { Plus, PartyPopper, ChevronDown } from '@lucide/svelte';

  let rescheduling = $state(false);

  const today = $derived(todayStr());

  // Move every overdue task to the picked day (or clear the date entirely).
  async function rescheduleOverdue(due_date) {
    const fields = due_date ? { due_date } : { due_date: null, due_time: null };
    try {
      await Promise.all(overdue.map((t) => updateTask(t.id, fields)));
    } catch (err) {
      toast(err.message);
    }
  }
  const overdue = $derived(
    data.tasks
      .filter((t) => t.due_date && t.due_date < today && !t.completed_at)
      .sort((a, b) => a.due_date.localeCompare(b.due_date) || a.priority - b.priority)
  );
  const dueToday = $derived(
    data.tasks
      .filter((t) => t.due_date === today && !t.completed_at)
      .sort((a, b) => (a.due_time ?? '99').localeCompare(b.due_time ?? '99') || a.priority - b.priority)
  );
</script>

<header class="mb-4">
  <h1 class="text-2xl font-bold tracking-tight">Today</h1>
  <p class="mt-0.5 text-sm text-zinc-400">{fullDayLabel(today)}</p>
</header>

{#if overdue.length}
  <section class="mb-6">
    <div class="flex items-baseline justify-between border-b border-zinc-200 pb-1.5 dark:border-zinc-800">
      <h2 class="text-sm font-semibold text-red-600 dark:text-red-400">
        Overdue · {overdue.length}
      </h2>
      <button
        onclick={() => (rescheduling = true)}
        class="flex items-center gap-0.5 text-sm font-semibold text-brand-600 hover:text-brand-700"
      >
        Reschedule
        <ChevronDown size={14} />
      </button>
    </div>
    <TaskList tasks={overdue} showProject />
  </section>
{/if}

{#if dueToday.length}
  <section>
    {#if overdue.length}
      <h2 class="border-b border-zinc-200 pb-1.5 text-sm font-semibold dark:border-zinc-800">Today</h2>
    {/if}
    <TaskList tasks={dueToday} showProject />
  </section>
{/if}

{#if !overdue.length && !dueToday.length}
  <div class="mt-16 flex flex-col items-center gap-3 text-center">
    <PartyPopper size={40} class="text-brand-400" />
    <p class="font-medium">You're all done for today</p>
    <p class="text-sm text-zinc-400">Enjoy the rest of your day.</p>
  </div>
{/if}

<button
  onclick={() => (ui.quickAdd = { due_date: today })}
  class="mt-3 flex items-center gap-2 text-sm text-zinc-500 transition hover:text-brand-600"
>
  <span class="flex h-5 w-5 items-center justify-center rounded-full text-brand-600"><Plus size={16} /></span>
  Add task
</button>

{#if rescheduling}
  <DatePicker
    title="Reschedule"
    allowsTime={false}
    onselect={rescheduleOverdue}
    onclose={() => (rescheduling = false)}
  />
{/if}
