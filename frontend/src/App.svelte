<script>
  import { data, ui, boot } from './lib/state.svelte.js';
  import Auth from './lib/components/Auth.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import BottomNav from './lib/components/BottomNav.svelte';
  import Toast from './lib/components/Toast.svelte';
  import QuickAdd from './lib/components/QuickAdd.svelte';
  import TaskModal from './lib/components/TaskModal.svelte';
  import Settings from './lib/components/Settings.svelte';
  import TodayView from './lib/views/Today.svelte';
  import UpcomingView from './lib/views/Upcoming.svelte';
  import InboxView from './lib/views/Inbox.svelte';
  import ProjectsView from './lib/views/Projects.svelte';
  import ProjectView from './lib/views/Project.svelte';
  import { Plus, RefreshCw, Settings as SettingsIcon } from '@lucide/svelte';
  import { getPullRefreshState } from './lib/pullRefresh.js';
  import { getQuickAddDefaults } from './lib/quickAdd.js';
  import { todayStr } from './lib/dates.js';

  boot();

  const PULL_REFRESH_THRESHOLD = 72;

  let scrollEl = $state(null);
  let pullStartY = $state(null);
  let pullStartX = $state(0);
  let pullDistance = $state(0);
  let pullReady = $state(false);
  let pullReloading = $state(false);

  $effect(() => {
    if (!scrollEl) return;
    // iOS only lets us suppress rubber-band overscroll from a non-passive touchmove listener.
    scrollEl.addEventListener('touchmove', movePullRefresh, { passive: false });
    return () => scrollEl.removeEventListener('touchmove', movePullRefresh);
  });

  function canPullRefresh() {
    return (
      data.user &&
      data.ready &&
      !ui.quickAdd &&
      !ui.openTaskId &&
      !ui.showSettings &&
      (scrollEl?.scrollTop ?? 0) <= 0
    );
  }

  function resetPullRefresh() {
    pullStartY = null;
    pullStartX = 0;
    pullDistance = 0;
    pullReady = false;
  }

  function startPullRefresh(e) {
    if (e.touches.length !== 1 || !canPullRefresh()) return;
    pullStartY = e.touches[0].clientY;
    pullStartX = e.touches[0].clientX;
  }

  function movePullRefresh(e) {
    if (pullStartY === null || e.touches.length !== 1) return;

    const touch = e.touches[0];
    const dy = touch.clientY - pullStartY;
    const dx = Math.abs(touch.clientX - pullStartX);
    if (dy <= 0 || dx > dy) {
      resetPullRefresh();
      return;
    }

    const state = getPullRefreshState({
      startY: pullStartY,
      currentY: touch.clientY,
      scrollTop: scrollEl?.scrollTop ?? 0,
      threshold: PULL_REFRESH_THRESHOLD
    });
    pullDistance = state.distance;
    pullReady = state.ready;

    if (pullDistance > 0) e.preventDefault();
  }

  function finishPullRefresh() {
    if (pullReady) {
      pullReloading = true;
      pullDistance = PULL_REFRESH_THRESHOLD;
      location.reload();
      return;
    }
    resetPullRefresh();
  }

  function openQuickAdd() {
    ui.quickAdd = getQuickAddDefaults({ view: ui.view, projectId: ui.projectId, today: todayStr() });
  }
</script>

{#if data.user === undefined}
  <div class="flex h-dvh items-center justify-center">
    <div class="text-2xl font-bold tracking-tight text-brand-600">TooDue</div>
  </div>
{:else if data.user === null}
  <Auth />
{:else}
  <div class="flex h-dvh bg-white dark:bg-zinc-950">
    <Sidebar />
    <main class="flex min-w-0 flex-1 flex-col">
      <!-- Mobile top bar: settings gear in the upper right -->
      <div
        class="flex flex-none items-center justify-end px-2 pb-0.5 md:hidden"
        style="padding-top: max(0.25rem, env(safe-area-inset-top))"
      >
        <button
          aria-label="Settings"
          onclick={() => (ui.showSettings = true)}
          class="rounded-lg p-2 text-zinc-500 transition active:bg-zinc-100 dark:text-zinc-400 dark:active:bg-zinc-800"
        >
          <SettingsIcon size={21} />
        </button>
      </div>
      <div
        bind:this={scrollEl}
        role="region"
        aria-label="Task content"
        ontouchstart={startPullRefresh}
        ontouchend={finishPullRefresh}
        ontouchcancel={resetPullRefresh}
        class="min-w-0 flex-1 overflow-y-auto px-4 pt-1 pb-28 sm:px-8 md:pt-8 md:pb-12"
        style="padding-bottom: max(7rem, env(safe-area-inset-bottom))"
      >
        <div
          class="pointer-events-none fixed left-1/2 z-50 flex items-center gap-2 rounded-full border border-zinc-200 bg-white/95 px-3 py-2 text-xs font-medium text-zinc-600 shadow-sm backdrop-blur transition dark:border-zinc-800 dark:bg-zinc-900/95 dark:text-zinc-300"
          style={`top: max(0.75rem, env(safe-area-inset-top)); transform: translate(-50%, ${Math.min(pullDistance, PULL_REFRESH_THRESHOLD)}px); opacity: ${pullDistance || pullReloading ? 1 : 0};`}
        >
          <RefreshCw size={14} class={pullReloading ? 'animate-spin' : ''} />
          {pullReloading ? 'Reloading…' : pullReady ? 'Release to reload' : 'Pull to reload'}
        </div>
        <div class="mx-auto w-full max-w-3xl">
          {#if data.offline || data.syncPending}
            <div class="mb-3 rounded-xl border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
              {data.syncPending ? 'Syncing offline changes…' : 'Offline mode — changes are saved here and will sync when you reconnect.'}
            </div>
          {/if}
          {#if !data.ready}
            <p class="mt-12 text-center text-sm text-zinc-400">Loading…</p>
          {:else if ui.view === 'today'}
            <TodayView />
          {:else if ui.view === 'upcoming'}
            <UpcomingView />
          {:else if ui.view === 'inbox'}
            <InboxView />
          {:else if ui.view === 'projects'}
            <ProjectsView />
          {:else if ui.view === 'project'}
            <ProjectView />
          {/if}
        </div>
      </div>
    </main>
  </div>

  <!-- Floating add button (mobile) -->
  <button
    aria-label="Add task"
    onclick={openQuickAdd}
    class="fixed right-5 bottom-24 z-30 flex h-14 w-14 items-center justify-center rounded-full bg-brand-600 text-white shadow-lg shadow-brand-600/30 transition active:scale-95 md:hidden"
  >
    <Plus size={28} />
  </button>

  <BottomNav />

  {#if ui.quickAdd}
    <QuickAdd />
  {/if}
  {#if ui.openTaskId}
    <TaskModal id={ui.openTaskId} />
  {/if}
  {#if ui.showSettings}
    <Settings />
  {/if}
{/if}

<Toast />
