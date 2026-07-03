<script>
  import { data, ui, boot } from './lib/state.svelte.js';
  import Auth from './lib/components/Auth.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import BottomNav from './lib/components/BottomNav.svelte';
  import Toast from './lib/components/Toast.svelte';
  import QuickAdd from './lib/components/QuickAdd.svelte';
  import TaskModal from './lib/components/TaskModal.svelte';
  import TodayView from './lib/views/Today.svelte';
  import UpcomingView from './lib/views/Upcoming.svelte';
  import InboxView from './lib/views/Inbox.svelte';
  import BrowseView from './lib/views/Browse.svelte';
  import ProjectView from './lib/views/Project.svelte';
  import { Plus } from '@lucide/svelte';

  boot();
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
    <main
      class="min-w-0 flex-1 overflow-y-auto px-4 pt-4 pb-28 sm:px-8 md:pt-8 md:pb-12"
      style="padding-bottom: max(7rem, env(safe-area-inset-bottom))"
    >
      <div class="mx-auto w-full max-w-3xl">
        {#if !data.ready}
          <p class="mt-12 text-center text-sm text-zinc-400">Loading…</p>
        {:else if ui.view === 'today'}
          <TodayView />
        {:else if ui.view === 'upcoming'}
          <UpcomingView />
        {:else if ui.view === 'inbox'}
          <InboxView />
        {:else if ui.view === 'browse'}
          <BrowseView />
        {:else if ui.view === 'project'}
          <ProjectView />
        {/if}
      </div>
    </main>
  </div>

  <!-- Floating add button (mobile) -->
  <button
    aria-label="Add task"
    onclick={() => (ui.quickAdd = {})}
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
{/if}

<Toast />
