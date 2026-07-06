<script>
  import { data, ui, navigate, inboxProject, addProject, toast } from '../state.svelte.js';
  import { todayStr } from '../dates.js';
  import { getQuickAddDefaults } from '../quickAdd.js';
  import BrandLogo from './BrandLogo.svelte';
  import {
    Inbox,
    CalendarCheck,
    CalendarDays,
    Plus,
    Hash,
    Users,
    Settings,
    ChevronRight
  } from '@lucide/svelte';

  const inbox = $derived(inboxProject());
  const inboxCount = $derived(
    data.tasks.filter((t) => t.project_id === inbox?.id && !t.completed_at).length
  );
  const todayCount = $derived(
    data.tasks.filter((t) => t.due_date && t.due_date <= todayStr() && !t.completed_at).length
  );
  const rootProjects = $derived(
    data.projects.filter((p) => !p.is_inbox && !p.parent_id)
  );

  function children(id) {
    return data.projects.filter((p) => p.parent_id === id);
  }

  let adding = $state(false);
  let newName = $state('');

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

  function openQuickAdd() {
    ui.quickAdd = getQuickAddDefaults({ view: ui.view, projectId: ui.projectId, today: todayStr() });
  }

  const navBtn =
    'flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-sm transition hover:bg-zinc-200/60 dark:hover:bg-zinc-800';
</script>

{#snippet projectRow(project, depth)}
  {@const active = ui.view === 'project' && ui.projectId === project.id}
  <button
    onclick={() => navigate('project', project.id)}
    class="{navBtn} {active ? 'bg-brand-50 font-medium text-brand-700 dark:bg-brand-950/60 dark:text-brand-300' : 'text-zinc-700 dark:text-zinc-300'}"
    style="padding-left: {10 + depth * 16}px"
  >
    <Hash size={16} class="flex-none opacity-60" />
    <span class="min-w-0 flex-1 truncate text-left">{project.name}</span>
    {#if project.members?.length > 1}
      <Users size={13} class="flex-none opacity-50" />
    {/if}
    {#if project.active_count}
      <span class="text-xs text-zinc-400">{project.active_count}</span>
    {/if}
  </button>
  {#each children(project.id) as child (child.id)}
    {@render projectRow(child, depth + 1)}
  {/each}
{/snippet}

<aside
  class="hidden w-72 flex-none flex-col border-r border-zinc-200 bg-zinc-50 md:flex dark:border-zinc-800 dark:bg-zinc-900/50"
>
  <div class="flex items-center gap-2 px-5 pt-5 pb-4">
    <BrandLogo size="sm" class="text-xl" />
  </div>

  <div class="px-3">
    <button
      onclick={openQuickAdd}
      class="mb-3 flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm font-semibold text-brand-600 transition hover:bg-brand-50 dark:hover:bg-brand-950/40"
    >
      <span class="flex h-6 w-6 items-center justify-center rounded-full bg-brand-600 text-white">
        <Plus size={16} />
      </span>
      Add task
    </button>

    <nav class="space-y-0.5">
      <button
        onclick={() => navigate('inbox')}
        class="{navBtn} {ui.view === 'inbox' ? 'bg-brand-50 font-medium text-brand-700 dark:bg-brand-950/60 dark:text-brand-300' : ''}"
      >
        <Inbox size={17} class="text-blue-500" />
        <span class="flex-1 text-left">Inbox</span>
        {#if inboxCount}<span class="text-xs text-zinc-400">{inboxCount}</span>{/if}
      </button>
      <button
        onclick={() => navigate('today')}
        class="{navBtn} {ui.view === 'today' ? 'bg-brand-50 font-medium text-brand-700 dark:bg-brand-950/60 dark:text-brand-300' : ''}"
      >
        <CalendarCheck size={17} class="text-emerald-600" />
        <span class="flex-1 text-left">Today</span>
        {#if todayCount}<span class="text-xs text-zinc-400">{todayCount}</span>{/if}
      </button>
      <button
        onclick={() => navigate('upcoming')}
        class="{navBtn} {ui.view === 'upcoming' ? 'bg-brand-50 font-medium text-brand-700 dark:bg-brand-950/60 dark:text-brand-300' : ''}"
      >
        <CalendarDays size={17} class="text-violet-500" />
        <span class="flex-1 text-left">Upcoming</span>
      </button>
    </nav>
  </div>

  <div class="mt-6 flex min-h-0 flex-1 flex-col px-3">
    <div class="flex items-center justify-between px-2.5 pb-1">
      <button
        onclick={() => navigate('projects')}
        title="Sort and nest projects"
        class="text-xs font-semibold tracking-wide text-zinc-500 uppercase hover:text-brand-600"
      >
        My Projects
      </button>
      <button
        aria-label="Add project"
        onclick={() => (adding = !adding)}
        class="rounded p-1 text-zinc-400 hover:bg-zinc-200/60 hover:text-zinc-600 dark:hover:bg-zinc-800"
      >
        <Plus size={15} />
      </button>
    </div>
    {#if adding}
      <form onsubmit={createProject} class="px-2.5 pb-2">
        <input
          bind:value={newName}
          placeholder="Project name"
          class="w-full rounded-lg border border-zinc-300 bg-white px-2.5 py-1.5 text-sm outline-none focus:border-brand-500 dark:border-zinc-700 dark:bg-zinc-900"
        />
      </form>
    {/if}
    <div class="min-h-0 flex-1 space-y-0.5 overflow-y-auto pb-4">
      {#each rootProjects as project (project.id)}
        {@render projectRow(project, 0)}
      {/each}
      {#if !rootProjects.length && !adding}
        <button
          onclick={() => (adding = true)}
          class="flex w-full items-center gap-1 px-2.5 py-1.5 text-sm text-zinc-400 hover:text-zinc-600"
        >
          Create a project <ChevronRight size={14} />
        </button>
      {/if}
    </div>
  </div>

  <div
    class="flex items-center gap-2 border-t border-zinc-200 px-4 py-3 dark:border-zinc-800"
  >
    <div
      class="flex h-8 w-8 flex-none items-center justify-center rounded-full bg-brand-100 text-sm font-bold text-brand-700 dark:bg-brand-950 dark:text-brand-300"
    >
      {data.user.name.slice(0, 1).toUpperCase()}
    </div>
    <span class="min-w-0 flex-1 truncate text-sm font-medium">{data.user.name}</span>
    <button
      aria-label="Settings"
      onclick={() => (ui.showSettings = true)}
      class="rounded p-1.5 text-zinc-400 hover:bg-zinc-200/60 hover:text-zinc-600 dark:hover:bg-zinc-800"
    >
      <Settings size={17} />
    </button>
  </div>
</aside>
