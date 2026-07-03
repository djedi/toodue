<script>
  import { ui, completeTask, projectById } from '../state.svelte.js';
  import { dayLabel, fmtTime, isOverdue, todayStr } from '../dates.js';
  import {
    Check,
    CalendarDays,
    Flag,
    MessageSquare,
    Paperclip,
    GitBranch,
    Hash
  } from '@lucide/svelte';

  let { task, showProject = false, depth = 0 } = $props();

  const checkColors = {
    1: 'border-red-500 text-red-500 hover:bg-red-50 dark:hover:bg-red-950/40',
    2: 'border-orange-400 text-orange-500 hover:bg-orange-50 dark:hover:bg-orange-950/40',
    3: 'border-blue-500 text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-950/40',
    4: 'border-zinc-300 text-zinc-400 hover:bg-zinc-100 dark:border-zinc-600 dark:hover:bg-zinc-800'
  };

  let checking = $state(false);
  const done = $derived(!!task.completed_at || checking);

  function toggle(e) {
    e.stopPropagation();
    if (task.completed_at) {
      completeTask(task, false);
      return;
    }
    if (checking) return;
    checking = true;
    // Brief pause so the check animation is visible before the row leaves the list.
    setTimeout(() => {
      completeTask(task, true);
      checking = false;
    }, 280);
  }

  const project = $derived(projectById(task.project_id));
  const dateColor = $derived(
    isOverdue(task.due_date)
      ? 'text-red-600 dark:text-red-400'
      : task.due_date === todayStr()
        ? 'text-emerald-600 dark:text-emerald-400'
        : 'text-zinc-500'
  );
</script>

<li style="padding-left: {depth * 28}px">
  <div
    role="button"
    tabindex="0"
    onclick={() => (ui.openTaskId = task.id)}
    onkeydown={(e) => e.key === 'Enter' && (ui.openTaskId = task.id)}
    class="group flex w-full cursor-pointer gap-3 border-b border-zinc-100 px-1 py-2.5 text-left dark:border-zinc-800/70"
  >
    <button aria-label={done ? 'Mark incomplete' : 'Complete task'} onclick={toggle} class="task-check {checkColors[task.priority]} {done ? 'bg-current/10' : ''}">
      {#if done}
        <Check size={13} strokeWidth={3} />
      {:else}
        <span class="opacity-0 transition group-hover:opacity-60"><Check size={13} strokeWidth={3} /></span>
      {/if}
    </button>

    <div class="min-w-0 flex-1">
      <div class="text-sm leading-snug {done ? 'text-zinc-400 line-through' : ''}">
        {task.name}
      </div>
      {#if task.description}
        <div class="mt-0.5 truncate text-xs text-zinc-400">{task.description}</div>
      {/if}
      {#if task.due_date || task.deadline || task.comment_count || task.attachment_count || task.subtask_count || (showProject && project)}
        <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs">
          {#if task.due_date}
            <span class="flex items-center gap-1 {dateColor}">
              <CalendarDays size={12} />
              {dayLabel(task.due_date)}{task.due_time ? ` ${fmtTime(task.due_time)}` : ''}
            </span>
          {/if}
          {#if task.deadline}
            <span class="flex items-center gap-1 {isOverdue(task.deadline) ? 'text-red-600 dark:text-red-400' : 'text-amber-600 dark:text-amber-500'}">
              <Flag size={12} />
              {dayLabel(task.deadline)}
            </span>
          {/if}
          {#if task.subtask_count}
            <span class="flex items-center gap-1 text-zinc-400">
              <GitBranch size={12} />
              {task.subtask_done_count}/{task.subtask_count}
            </span>
          {/if}
          {#if task.comment_count}
            <span class="flex items-center gap-1 text-zinc-400">
              <MessageSquare size={12} />
              {task.comment_count}
            </span>
          {/if}
          {#if task.attachment_count}
            <span class="flex items-center gap-1 text-zinc-400">
              <Paperclip size={12} />
              {task.attachment_count}
            </span>
          {/if}
          {#if showProject && project}
            <span class="ml-auto flex items-center gap-0.5 text-zinc-400">
              {project.is_inbox ? 'Inbox' : project.name}
              <Hash size={11} />
            </span>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</li>
