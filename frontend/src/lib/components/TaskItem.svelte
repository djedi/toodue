<script>
  import { ui, completeTask, deleteTask, projectById, toast } from '../state.svelte.js';
  import { dayLabel, fmtTime, isOverdue, todayStr } from '../dates.js';
  import { repeatLabel } from '../recurrence.js';
  import { untrack } from 'svelte';
  import {
    Check,
    CalendarDays,
    Flag,
    MessageSquare,
    Paperclip,
    GitBranch,
    Repeat2,
    Hash,
    Trash2
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

  /* ---------- swipe-to-delete (touch only) ---------- */
  const ACTION_W = 72; // px, width of the revealed trash button
  let offset = $state(0); // current translateX of the row, 0..-ACTION_W
  let dragging = $state(false);
  let deleting = $state(false);
  const revealed = $derived(ui.swipedTaskId === task.id);
  let start = null; // { x, y, offset, axis: null | 'x' | 'y' }

  // Keep the row position in sync with the shared "revealed" state (e.g. another row was swiped open).
  $effect(() => {
    if (dragging) return;
    const target = revealed ? -ACTION_W : 0;
    if (offset !== target) offset = target;
  });

  // Collapse any revealed row if the viewport grows past the md breakpoint (e.g. rotation),
  // where the delete action is hidden.
  $effect(() => {
    const mq = matchMedia('(max-width: 767px)');
    const onChange = () => {
      if (mq.matches) return;
      start = null;
      dragging = false;
      if (revealed) ui.swipedTaskId = null;
    };
    onChange(); // also covers mounting at desktop width with stale swipe state
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });

  function onTouchStart(e) {
    // The delete action is only rendered below the md breakpoint; don't let wider touch screens swipe.
    if (e.touches.length !== 1 || !matchMedia('(max-width: 767px)').matches) return;
    const t = e.touches[0];
    start = { x: t.clientX, y: t.clientY, offset: revealed ? -ACTION_W : 0, axis: null };
  }

  function onTouchMove(e) {
    if (!start || e.touches.length !== 1) return;
    const t = e.touches[0];
    const dx = t.clientX - start.x;
    const dy = t.clientY - start.y;
    if (!start.axis) {
      if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return;
      start.axis = Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
      if (start.axis === 'x') dragging = true;
    }
    if (start.axis !== 'x') return;
    // Allow a little rubber-band past the button, never past the left edge.
    offset = Math.max(-ACTION_W - 24, Math.min(0, start.offset + dx));
  }

  function onTouchEnd() {
    if (!start) return;
    const wasDragging = start.axis === 'x';
    start = null;
    if (!wasDragging) return;
    dragging = false;
    if (offset < -ACTION_W / 2) {
      offset = -ACTION_W;
      ui.swipedTaskId = task.id;
    } else {
      offset = 0;
      if (revealed) ui.swipedTaskId = null;
    }
  }

  // A revealed row that leaves the DOM (view change, list refresh, delete) releases the shared state,
  // so a non-null ui.swipedTaskId always refers to a mounted row.
  $effect(() => {
    return () => {
      if (untrack(() => ui.swipedTaskId) === task.id) ui.swipedTaskId = null;
    };
  });

  function open() {
    // A tap while a row is swiped open just closes the action instead of opening a task.
    if (ui.swipedTaskId !== null) {
      ui.swipedTaskId = null;
      return;
    }
    ui.openTaskId = task.id;
  }

  async function remove(e) {
    e.stopPropagation();
    if (deleting) return;
    deleting = true;
    try {
      await deleteTask(task.id); // clears ui.swipedTaskId for this row itself
    } catch (err) {
      deleting = false;
      toast(err.message);
    }
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

<li style="padding-left: {depth * 28}px" class="relative overflow-hidden">
  <!-- Swipe action (mobile only): revealed by dragging the row left. Only in the DOM while visible. -->
  {#if revealed || offset !== 0}
    <button
      type="button"
      aria-label="Delete task: {task.name}"
      aria-hidden={!revealed}
      tabindex={revealed ? 0 : -1}
      onclick={remove}
      disabled={deleting}
      class="absolute inset-y-0 right-0 flex items-center justify-center bg-red-500 text-white disabled:opacity-60 md:hidden"
      style="width: {ACTION_W}px"
    >
      <Trash2 size={20} />
    </button>
  {/if}
  <div
    role="button"
    tabindex="0"
    onclick={open}
    onkeydown={(e) => e.key === 'Enter' && (ui.openTaskId = task.id)}
    ontouchstart={onTouchStart}
    ontouchmove={onTouchMove}
    ontouchend={onTouchEnd}
    ontouchcancel={onTouchEnd}
    style="transform: translateX({offset}px); transition: {dragging ? 'none' : 'transform 180ms ease'}; touch-action: pan-y pinch-zoom"
    class="group relative flex w-full cursor-pointer gap-3 border-b border-zinc-100 bg-white px-1 py-2.5 text-left dark:border-zinc-800/70 dark:bg-zinc-950"
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
      {#if task.due_date || task.repeat_rule || task.deadline || task.comment_count || task.attachment_count || task.subtask_count || (showProject && project)}
        <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs">
          {#if task.due_date}
            <span class="flex items-center gap-1 {dateColor}">
              <CalendarDays size={12} />
              {dayLabel(task.due_date)}{task.due_time ? ` ${fmtTime(task.due_time)}` : ''}
            </span>
          {/if}
          {#if task.repeat_rule}
            <span class="flex items-center gap-1 text-brand-600 dark:text-brand-400" title={repeatLabel(task.repeat_rule)}>
              <Repeat2 size={12} />
              {repeatLabel(task.repeat_rule)}
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
