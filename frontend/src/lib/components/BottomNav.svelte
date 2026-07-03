<script>
  import { ui, data, navigate, inboxProject } from '../state.svelte.js';
  import { todayStr } from '../dates.js';
  import { Inbox, CalendarCheck, CalendarDays, LayoutGrid } from '@lucide/svelte';

  const tabs = [
    { view: 'inbox', label: 'Inbox', icon: Inbox },
    { view: 'today', label: 'Today', icon: CalendarCheck },
    { view: 'upcoming', label: 'Upcoming', icon: CalendarDays },
    { view: 'browse', label: 'Browse', icon: LayoutGrid }
  ];

  const counts = $derived({
    inbox: data.tasks.filter((t) => t.project_id === inboxProject()?.id && !t.completed_at).length,
    today: data.tasks.filter((t) => t.due_date && t.due_date <= todayStr() && !t.completed_at)
      .length
  });
</script>

<nav
  class="fixed inset-x-0 bottom-0 z-20 border-t border-zinc-200 bg-white/95 backdrop-blur dark:border-zinc-800 dark:bg-zinc-950/95 md:hidden"
  style="padding-bottom: env(safe-area-inset-bottom)"
>
  <div class="flex">
    {#each tabs as tab (tab.view)}
      {@const active = ui.view === tab.view}
      <button
        onclick={() => navigate(tab.view)}
        class="relative flex flex-1 flex-col items-center gap-0.5 py-2 text-[11px] font-medium transition {active
          ? 'text-brand-600'
          : 'text-zinc-500 dark:text-zinc-400'}"
      >
        <div class="relative">
          <tab.icon size={22} strokeWidth={active ? 2.4 : 2} />
          {#if counts[tab.view]}
            <span
              class="absolute -top-1.5 -right-2.5 rounded-full bg-brand-600 px-1 text-[9px] leading-3.5 font-bold text-white"
              >{counts[tab.view]}</span
            >
          {/if}
        </div>
        {tab.label}
      </button>
    {/each}
  </div>
</nav>
