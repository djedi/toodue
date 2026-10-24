<script>
  import { api } from '../api.js';
  import { data, ui, bus, projectById, navigate, toast, isTaskDeleted, startSelection, endSelection } from '../state.svelte.js';
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
  let completedGen = 0; // bumped on project change so stale responses are ignored

  $effect(() => {
    // Reset per-project UI state when navigating between projects.
    ui.projectId;
    showCompleted = false;
    completedTasks = [];
    removedCompleted = new Map();
    completedGen++;
  });

  // Completed rows removed optimistically, kept so a failed delete can put them back.
  let removedCompleted = new Map();
  let loadingCompleted = false;

  // A mutation landed while the completed list was loading: that response is now stale, fetch again.
  function invalidateCompletedLoad() {
    if (!loadingCompleted) return;
    completedGen++;
    loadCompleted();
  }

  $effect(() => {
    const onDeleted = (e) => {
      invalidateCompletedLoad();
      const id = e.detail.id;
      const gone = completedTasks.filter((t) => t.id === id || t.parent_id === id);
      if (gone.length) removedCompleted.set(id, gone);
      completedTasks = completedTasks.filter((t) => t.id !== id && t.parent_id !== id);
    };
    const onDeleteFailed = (e) => {
      const id = e.detail.id;
      const gone = removedCompleted.get(id);
      removedCompleted.delete(id);
      if (!gone) return;
      const present = new Set(completedTasks.map((t) => t.id));
      completedTasks = [...completedTasks, ...gone.filter((t) => !present.has(t.id))];
    };
    // Confirmed edits reach completed rows too: moved or reopened tasks leave the list, others refresh.
    const onUpdated = (e) => {
      invalidateCompletedLoad();
      const t = e.detail;
      if (t.project_id !== ui.projectId) {
        // Moved away: the server moves its subtasks with it.
        completedTasks = completedTasks.filter((x) => x.id !== t.id && x.parent_id !== t.id);
        return;
      }
      if (!completedTasks.some((x) => x.id === t.id)) return;
      if (!t.completed_at) completedTasks = completedTasks.filter((x) => x.id !== t.id);
      else completedTasks = completedTasks.map((x) => (x.id === t.id ? t : x));
    };
    bus.addEventListener('task-deleted', onDeleted);
    bus.addEventListener('task-delete-failed', onDeleteFailed);
    bus.addEventListener('task-updated', onUpdated);
    return () => {
      bus.removeEventListener('task-deleted', onDeleted);
      bus.removeEventListener('task-delete-failed', onDeleteFailed);
      bus.removeEventListener('task-updated', onUpdated);
    };
  });

  async function loadCompleted() {
    const gen = completedGen;
    loadingCompleted = true;
    try {
      const rows = await api.get(`/tasks?project_id=${ui.projectId}&completed=true`);
      if (gen !== completedGen) return;
      // Rows deleted while this request was in flight are held back, but kept for rollback.
      for (const t of rows) {
        const key = isTaskDeleted(t.id) ? t.id : isTaskDeleted(t.parent_id) ? t.parent_id : null;
        if (key === null) continue;
        const held = removedCompleted.get(key) ?? [];
        if (!held.some((h) => h.id === t.id)) removedCompleted.set(key, [...held, t]);
      }
      completedTasks = rows.filter((t) => !isTaskDeleted(t.id) && !isTaskDeleted(t.parent_id));
    } catch (err) {
      if (gen === completedGen) toast(err.message);
    } finally {
      if (gen === completedGen) loadingCompleted = false;
    }
  }

  async function toggleCompleted() {
    showCompleted = !showCompleted;
    if (showCompleted) await loadCompleted();
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
      <button
      onclick={() => (ui.selecting ? endSelection() : startSelection())}
      class="rounded-lg border border-zinc-200 px-2.5 py-1.5 text-xs font-medium text-zinc-600 hover:border-zinc-300 dark:border-zinc-700 dark:text-zinc-300"
    >
      {ui.selecting ? 'Done' : 'Select'}
    </button>
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
