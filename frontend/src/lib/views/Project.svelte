<script>
  import { api } from '../api.js';
  import { data, ui, projectById, navigate, toast } from '../state.svelte.js';
  import TaskList from '../components/TaskList.svelte';
  import TaskItem from '../components/TaskItem.svelte';
  import ProjectDialog from '../components/ProjectDialog.svelte';
  import { Hash, Plus, Users, Settings2, ChevronRight } from '@lucide/svelte';

  const project = $derived(projectById(ui.projectId));
  const tasks = $derived(
    data.tasks
      .filter((t) => t.project_id === ui.projectId && !t.completed_at)
      .sort((a, b) => a.sort_order - b.sort_order || a.id - b.id)
  );
  const subProjects = $derived(data.projects.filter((p) => p.parent_id === ui.projectId));

  let showSettings = $state(false);
  let showCompleted = $state(false);
  let completedTasks = $state([]);

  $effect(() => {
    // Reset per-project UI state when navigating between projects.
    ui.projectId;
    showCompleted = false;
    completedTasks = [];
  });

  async function toggleCompleted() {
    showCompleted = !showCompleted;
    if (showCompleted) {
      try {
        completedTasks = await api.get(`/tasks?project_id=${ui.projectId}&completed=true`);
      } catch (err) {
        toast(err.message);
      }
    }
  }
</script>

{#if !project}
  <p class="mt-12 text-center text-sm text-zinc-400">Project not found.</p>
{:else}
  <header class="mb-4">
    <div class="flex items-center gap-2">
      <Hash size={22} class="text-zinc-400" />
      <h1 class="min-w-0 flex-1 truncate text-2xl font-bold tracking-tight">
        {project.is_inbox ? 'Inbox' : project.name}
      </h1>
      {#if !project.is_inbox}
        <button
          aria-label="Project settings"
          onclick={() => (showSettings = true)}
          class="flex items-center gap-1.5 rounded-lg border border-zinc-200 px-2.5 py-1.5 text-xs font-medium text-zinc-600 hover:border-zinc-300 dark:border-zinc-700 dark:text-zinc-300"
        >
          {#if project.members?.length > 1}
            <Users size={14} />
            {project.members.length}
          {:else}
            <Settings2 size={14} />
          {/if}
        </button>
      {/if}
    </div>
    {#if project.members?.length > 1}
      <p class="mt-1 text-xs text-zinc-400">
        Shared with {project.members
          .filter((m) => m.id !== data.user.id)
          .map((m) => m.name)
          .join(', ')}
      </p>
    {/if}
  </header>

  {#if subProjects.length}
    <div class="mb-4 flex flex-wrap gap-2">
      {#each subProjects as sp (sp.id)}
        <button
          onclick={() => navigate('project', sp.id)}
          class="flex items-center gap-1 rounded-full border border-zinc-200 px-3 py-1 text-xs font-medium text-zinc-600 hover:border-brand-400 hover:text-brand-600 dark:border-zinc-700 dark:text-zinc-300"
        >
          <Hash size={12} />
          {sp.name}
          <ChevronRight size={12} />
        </button>
      {/each}
    </div>
  {/if}

  <TaskList {tasks} nest />

  <button
    onclick={() => (ui.quickAdd = { project_id: ui.projectId })}
    class="mt-3 flex items-center gap-2 text-sm text-zinc-500 transition hover:text-brand-600"
  >
    <span class="flex h-5 w-5 items-center justify-center rounded-full text-brand-600"><Plus size={16} /></span>
    Add task
  </button>

  <div class="mt-8 border-t border-zinc-100 pt-3 dark:border-zinc-800">
    <button onclick={toggleCompleted} class="text-xs font-medium text-zinc-400 hover:text-zinc-600">
      {showCompleted ? 'Hide completed' : 'Show completed'}
    </button>
    {#if showCompleted}
      {#if completedTasks.length}
        <ul class="mt-1 opacity-70">
          {#each completedTasks as task (task.id)}
            <TaskItem {task} />
          {/each}
        </ul>
      {:else}
        <p class="mt-2 text-xs text-zinc-400">No completed tasks yet.</p>
      {/if}
    {/if}
  </div>

  {#if showSettings}
    <ProjectDialog {project} onclose={() => (showSettings = false)} />
  {/if}
{/if}
