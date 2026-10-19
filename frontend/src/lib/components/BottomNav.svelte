<script>
  import { untrack } from 'svelte';
  import { ui, data, navigate, inboxProject } from '../state.svelte.js';
  import { todayStr } from '../dates.js';
  import { Inbox, CalendarCheck, CalendarDays, LayoutGrid } from '@lucide/svelte';
  import { nativeTabsEnabled, NativeTabs, watchOverlays } from '../nativeTabs.js';

  // `symbol` is the SF Symbol the native iOS tab bar uses.
  const tabs = [
    { view: 'inbox', label: 'Inbox', icon: Inbox, symbol: 'tray' },
    { view: 'today', label: 'Today', icon: CalendarCheck, symbol: 'calendar.badge.checkmark' },
    { view: 'upcoming', label: 'Upcoming', icon: CalendarDays, symbol: 'calendar' },
    { view: 'projects', label: 'Projects', icon: LayoutGrid, symbol: 'square.grid.2x2' }
  ];

  const counts = $derived({
    inbox: data.tasks.filter((t) => t.project_id === inboxProject()?.id && !t.completed_at).length,
    today: data.tasks.filter((t) => t.due_date && t.due_date <= todayStr() && !t.completed_at)
      .length
  });

  let overlayOpen = $state(false);
  let narrow = $state(true);
  let useNative = $state(nativeTabsEnabled);
  const fallBack = () => (useNative = false);

  if (nativeTabsEnabled) {
    $effect(() => {
      const handle = NativeTabs.addListener('tabSelected', ({ id }) => navigate(id));
      const stopWatching = watchOverlays((open) => (overlayOpen = open));
      // Mirror the web nav's md:hidden breakpoint (iPad uses the sidebar).
      const mq = matchMedia('(max-width: 767px)');
      const onMq = () => (narrow = mq.matches);
      onMq();
      mq.addEventListener('change', onMq);
      return () => {
        handle.then((h) => h.remove());
        stopWatching();
        mq.removeEventListener('change', onMq);
        NativeTabs.hide().catch(fallBack);
      };
    });

    $effect(() => {
      // Re-send the accent when the color scheme changes.
      void ui.colorScheme;
      if (narrow && !overlayOpen) {
        NativeTabs.show({
          tabs: tabs.map((t) => ({ id: t.view, label: t.label, icon: t.symbol })),
          selected: untrack(() => ui.view),
          tint: getComputedStyle(document.documentElement).getPropertyValue('--color-brand-600').trim()
        }).catch(fallBack);
      } else {
        NativeTabs.hide().catch(fallBack);
      }
    });

    $effect(() => {
      NativeTabs.select({ id: ui.view }).catch(fallBack);
    });

    $effect(() => {
      NativeTabs.setBadges({ badges: { ...counts } }).catch(fallBack);
    });
  }
</script>

{#if !useNative}
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
{/if}
