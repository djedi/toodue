<script>
  import { data, navigate, addProject, toast } from '../state.svelte.js';
  import { Hash, Plus, Users, ChevronRight } from '@lucide/svelte';

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

<header class="mb-4 flex items-center justify-between">
  <h1 class="text-2xl font-bold tracking-tight">Projects</h1>
  <button
    aria-label="Add project"
    onclick={() => (adding = !adding)}
    class="rounded p-1 text-zinc-400 hover:text-brand-600"
  >
    <Plus size={20} />
  </button>
</header>

{#if adding}
  <form onsubmit={createProject} class="mb-2">
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
