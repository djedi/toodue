<script>
  import TaskItem from './TaskItem.svelte';

  let { tasks = [], showProject = false, nest = false } = $props();

  const parents = $derived(nest ? tasks.filter((t) => !t.parent_id) : tasks);

  function childrenOf(id) {
    return tasks.filter((t) => t.parent_id === id);
  }
</script>

<ul>
  {#each parents as task (task.id)}
    <TaskItem {task} {showProject} />
    {#if nest}
      {#each childrenOf(task.id) as child (child.id)}
        <TaskItem task={child} {showProject} depth={1} />
      {/each}
    {/if}
  {/each}
</ul>
