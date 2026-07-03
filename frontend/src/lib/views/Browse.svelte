<script>
  import { api } from '../api.js';
  import { data, navigate, addProject, refresh, signOut, toast } from '../state.svelte.js';
  import ThemeSwitcher from '../components/ThemeSwitcher.svelte';
  import {
    Hash,
    Plus,
    Users,
    ChevronRight,
    CalendarPlus,
    LogOut,
    Copy,
    FileUp
  } from '@lucide/svelte';

  const rootProjects = $derived(data.projects.filter((p) => !p.is_inbox && !p.parent_id));

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

  let importInput = $state(null);
  let importing = $state(false);

  async function importTodoist(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    importing = true;
    const form = new FormData();
    form.append('file', file);
    try {
      const r = await api.post('/import/todoist', form);
      await refresh();
      let msg = `Imported ${r.tasks} tasks across ${r.projects} projects`;
      if (r.comments) msg += `, ${r.comments} comments`;
      if (r.recurring_converted) {
        msg += ` (${r.recurring_converted} recurring dates became one-time)`;
      }
      toast(msg);
    } catch (err) {
      toast(err.message);
    } finally {
      importing = false;
      e.target.value = '';
    }
  }

  async function copyCalendarUrl() {
    try {
      const { url } = await api.get('/me/calendar');
      const absolute = location.origin + url;
      await navigator.clipboard.writeText(absolute);
      toast('Calendar feed URL copied — subscribe to it in Google Calendar or Fantastical');
    } catch (err) {
      toast(err.message);
    }
  }
</script>

{#snippet projectRow(project, depth)}
  <button
    onclick={() => navigate('project', project.id)}
    class="flex w-full items-center gap-3 border-b border-zinc-100 py-3 text-left dark:border-zinc-800/70"
    style="padding-left: {depth * 24}px"
  >
    <Hash size={17} class="flex-none text-zinc-400" />
    <span class="min-w-0 flex-1 truncate text-sm font-medium">{project.name}</span>
    {#if project.members?.length > 1}
      <Users size={14} class="flex-none text-zinc-400" />
    {/if}
    {#if project.active_count}
      <span class="text-xs text-zinc-400">{project.active_count}</span>
    {/if}
    <ChevronRight size={16} class="text-zinc-300 dark:text-zinc-600" />
  </button>
  {#each children(project.id) as child (child.id)}
    {@render projectRow(child, depth + 1)}
  {/each}
{/snippet}

<header class="mb-4 flex items-center gap-3">
  <div
    class="flex h-10 w-10 items-center justify-center rounded-full bg-brand-100 text-base font-bold text-brand-700 dark:bg-brand-950 dark:text-brand-300"
  >
    {data.user.name.slice(0, 1).toUpperCase()}
  </div>
  <div class="min-w-0">
    <h1 class="truncate text-lg font-bold tracking-tight">{data.user.name}</h1>
    <p class="truncate text-xs text-zinc-400">{data.user.email}</p>
  </div>
</header>

<section class="mt-6">
  <div class="flex items-center justify-between">
    <h2 class="text-sm font-semibold tracking-wide text-zinc-500 uppercase">My Projects</h2>
    <button
      aria-label="Add project"
      onclick={() => (adding = !adding)}
      class="rounded p-1 text-zinc-400 hover:text-brand-600"
    >
      <Plus size={18} />
    </button>
  </div>
  {#if adding}
    <form onsubmit={createProject} class="mt-2">
      <input
        bind:value={newName}
        placeholder="Project name"
        class="w-full rounded-lg border border-zinc-300 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
      />
    </form>
  {/if}
  <div class="mt-1">
    {#each rootProjects as project (project.id)}
      {@render projectRow(project, 0)}
    {:else}
      {#if !adding}
        <p class="py-3 text-sm text-zinc-400">No projects yet — create one to organize your tasks.</p>
      {/if}
    {/each}
  </div>
</section>

<section class="mt-8">
  <h2 class="text-sm font-semibold tracking-wide text-zinc-500 uppercase">Settings</h2>
  <div class="mt-2 space-y-3">
    <div class="flex items-center justify-between rounded-xl border border-zinc-200 px-4 py-3 dark:border-zinc-800">
      <span class="text-sm font-medium">Theme</span>
      <ThemeSwitcher />
    </div>
    <button
      onclick={copyCalendarUrl}
      class="flex w-full items-center gap-3 rounded-xl border border-zinc-200 px-4 py-3 text-left dark:border-zinc-800"
    >
      <CalendarPlus size={18} class="text-zinc-400" />
      <span class="flex-1">
        <span class="block text-sm font-medium">Calendar feed</span>
        <span class="block text-xs text-zinc-400">
          Copy an iCal URL for Google Calendar or Fantastical
        </span>
      </span>
      <Copy size={15} class="text-zinc-400" />
    </button>
    <button
      onclick={() => importInput.click()}
      disabled={importing}
      class="flex w-full items-center gap-3 rounded-xl border border-zinc-200 px-4 py-3 text-left disabled:opacity-50 dark:border-zinc-800"
    >
      <FileUp size={18} class="text-zinc-400" />
      <span class="flex-1">
        <span class="block text-sm font-medium">
          {importing ? 'Importing…' : 'Import from Todoist'}
        </span>
        <span class="block text-xs text-zinc-400">
          Upload a Todoist backup (.zip) to bring in your projects and tasks
        </span>
      </span>
      <ChevronRight size={15} class="text-zinc-400" />
    </button>
    <input bind:this={importInput} type="file" accept=".zip" class="hidden" onchange={importTodoist} />
    <button
      onclick={signOut}
      class="flex w-full items-center gap-3 rounded-xl border border-zinc-200 px-4 py-3 text-left text-sm font-medium text-red-600 dark:border-zinc-800 dark:text-red-400"
    >
      <LogOut size={18} /> Log out
    </button>
  </div>
</section>
