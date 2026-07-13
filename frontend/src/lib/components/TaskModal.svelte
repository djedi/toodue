<script>
  import { api } from '../api.js';
  import {
    data,
    ui,
    bus,
    updateTask,
    deleteTask,
    addTask,
    completeTask,
    projectById,
    toast
  } from '../state.svelte.js';
  import { fmtTimestamp, fmtTime, dayLabel, todayStr } from '../dates.js';
  import { repeatOptions } from '../recurrence.js';
  import DatePicker from './DatePicker.svelte';
  import {
    X,
    Check,
    Trash2,
    Paperclip,
    Send,
    Plus,
    CalendarDays,
    Flag,
    Repeat2,
    Download,
    Hash
  } from '@lucide/svelte';

  let { id } = $props();

  let detail = $state(null);
  let name = $state('');
  let description = $state('');
  let newSubtask = $state('');
  let newComment = $state('');
  let fileInput = $state(null);
  let pickerFor = $state(null); // 'due' | 'deadline'

  const task = $derived(detail?.task);
  const project = $derived(task ? projectById(task.project_id) : null);

  async function load() {
    try {
      detail = await api.get(`/tasks/${id}`);
      name = detail.task.name;
      description = detail.task.description;
    } catch (err) {
      const cached = data.tasks.find((t) => t.id === id);
      if (cached && data.offline) {
        detail = {
          task: cached,
          subtasks: data.tasks.filter((t) => t.parent_id === id),
          comments: [],
          attachments: []
        };
        name = cached.name;
        description = cached.description;
        return;
      }
      toast(err.message);
      close();
    }
  }

  $effect(() => {
    id;
    load();
  });

  // Refresh the open task when a server event touches it.
  $effect(() => {
    const handler = (e) => {
      const { type, data: d } = e.detail;
      if (!detail) return;
      const relevant =
        d?.id === id ||
        d?.task_id === id ||
        d?.parent_id === id ||
        (type === 'tasks.refresh');
      if (relevant) load();
    };
    bus.addEventListener('server-event', handler);
    return () => bus.removeEventListener('server-event', handler);
  });

  function close() {
    ui.openTaskId = null;
  }

  function onkeydown(e) {
    if (e.key === 'Escape' && !pickerFor) close();
  }

  async function save(fields) {
    try {
      await updateTask(id, fields);
      await load();
    } catch (err) {
      toast(err.message);
    }
  }

  async function toggleDone() {
    const completed = !task.completed_at;
    await save({ completed, ...(completed ? { completed_on: todayStr() } : {}) });
  }

  async function removeTask() {
    if (!confirm('Delete this task?')) return;
    try {
      await deleteTask(id);
    } catch (err) {
      toast(err.message);
    }
  }

  async function submitSubtask(e) {
    e.preventDefault();
    if (!newSubtask.trim()) return;
    try {
      await addTask({ parent_id: id, name: newSubtask });
      newSubtask = '';
      await load();
    } catch (err) {
      toast(err.message);
    }
  }

  async function submitComment(e) {
    e.preventDefault();
    if (!newComment.trim()) return;
    try {
      await api.post(`/tasks/${id}/comments`, { body: newComment });
      newComment = '';
      await load();
    } catch (err) {
      toast(err.message);
    }
  }

  async function deleteComment(cid) {
    try {
      await api.del(`/comments/${cid}`);
      await load();
    } catch (err) {
      toast(err.message);
    }
  }

  async function uploadFile(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    const form = new FormData();
    form.append('file', file);
    try {
      await api.post(`/tasks/${id}/attachments`, form);
      await load();
    } catch (err) {
      toast(err.message);
    } finally {
      e.target.value = '';
    }
  }

  async function deleteAttachment(aid) {
    try {
      await api.del(`/attachments/${aid}`);
      await load();
    } catch (err) {
      toast(err.message);
    }
  }

  function fmtSize(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  const checkColors = {
    1: 'border-red-500 text-red-500',
    2: 'border-orange-400 text-orange-500',
    3: 'border-blue-500 text-blue-500',
    4: 'border-zinc-300 text-zinc-400 dark:border-zinc-600'
  };

  const fieldLabel = 'text-[11px] font-semibold tracking-wide text-zinc-400 uppercase';
  const fieldInput =
    'w-full rounded-lg border border-zinc-200 bg-transparent px-2 py-1.5 text-sm outline-none focus:border-brand-500 dark:border-zinc-700';
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-40 flex items-stretch justify-center bg-black/40 sm:items-center sm:p-6"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && close()}
>
  <div
    class="flex w-full flex-col overflow-hidden bg-white sm:max-h-[85vh] sm:max-w-2xl sm:rounded-2xl sm:shadow-xl dark:bg-zinc-900"
  >
    {#if !detail}
      <p class="p-8 text-center text-sm text-zinc-400">Loading…</p>
    {:else}
      <!-- header -->
      <div class="flex items-center gap-2 border-b border-zinc-100 px-4 py-3 dark:border-zinc-800">
        <span class="flex items-center gap-1 text-sm text-zinc-500">
          <Hash size={14} />
          {project?.is_inbox ? 'Inbox' : (project?.name ?? '…')}
        </span>
        <div class="ml-auto flex items-center gap-1">
          <button
            aria-label="Delete task"
            onclick={removeTask}
            class="rounded p-1.5 text-zinc-400 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/40"
          >
            <Trash2 size={17} />
          </button>
          <button aria-label="Close" onclick={close} class="rounded p-1.5 text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-800">
            <X size={18} />
          </button>
        </div>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-6">
        <!-- title -->
        <div class="flex items-start gap-3">
          <button aria-label="Toggle complete" onclick={toggleDone} class="task-check {checkColors[task.priority]} {task.completed_at ? 'bg-current/10' : ''}">
            {#if task.completed_at}<Check size={13} strokeWidth={3} />{/if}
          </button>
          <input
            bind:value={name}
            onblur={() => name.trim() && name !== task.name && save({ name })}
            class="w-full bg-transparent text-lg font-semibold outline-none {task.completed_at ? 'text-zinc-400 line-through' : ''}"
          />
        </div>
        <textarea
          bind:value={description}
          onblur={() => description !== task.description && save({ description })}
          placeholder="Add a description…"
          rows="2"
          class="mt-2 ml-8 w-[calc(100%-2rem)] resize-none bg-transparent text-sm text-zinc-600 outline-none placeholder:text-zinc-400 dark:text-zinc-300"
        ></textarea>

        <!-- fields -->
        <div class="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div>
            <div class={fieldLabel}><CalendarDays size={11} class="mr-1 inline" />Date</div>
            <button
              onclick={() => (pickerFor = 'due')}
              class="{fieldInput} mt-1 text-left {task.due_date ? '' : 'text-zinc-400'}"
            >
              {task.due_date
                ? `${dayLabel(task.due_date)}${task.due_time ? ` · ${fmtTime(task.due_time)}` : ''}`
                : 'None'}
            </button>
          </div>
          <div>
            <div class={fieldLabel}><Flag size={11} class="mr-1 inline" />Deadline</div>
            <button
              onclick={() => (pickerFor = 'deadline')}
              class="{fieldInput} mt-1 text-left {task.deadline ? '' : 'text-zinc-400'}"
            >
              {task.deadline ? dayLabel(task.deadline) : 'None'}
            </button>
          </div>
          <div>
            <div class={fieldLabel}><Repeat2 size={11} class="mr-1 inline" />Repeat</div>
            <select
              value={task.repeat_rule ?? ''}
              disabled={!task.due_date || !!task.parent_id}
              onchange={(e) => save({ repeat_rule: e.target.value || null })}
              class="{fieldInput} mt-1 disabled:opacity-40"
            >
              {#each repeatOptions as option}
                <option value={option.value}>{option.label}</option>
              {/each}
            </select>
          </div>
          <div>
            <div class={fieldLabel}>Priority</div>
            <select
              value={task.priority}
              onchange={(e) => save({ priority: Number(e.target.value) })}
              class="{fieldInput} mt-1"
            >
              <option value={1}>P1 · Urgent</option>
              <option value={2}>P2 · High</option>
              <option value={3}>P3 · Medium</option>
              <option value={4}>P4 · Normal</option>
            </select>
          </div>
        </div>

        <div class="mt-3">
          <div class={fieldLabel}>Project</div>
          <select
            value={task.project_id}
            onchange={(e) => save({ project_id: Number(e.target.value) })}
            class="{fieldInput} mt-1 sm:w-64"
          >
            {#each data.projects as p (p.id)}
              <option value={p.id}>{p.is_inbox ? 'Inbox' : p.name}</option>
            {/each}
          </select>
        </div>

        <!-- subtasks -->
        {#if !task.parent_id}
          <div class="mt-6">
            <h3 class="text-sm font-semibold">
              Sub-tasks
              {#if detail.subtasks.length}
                <span class="ml-1 text-xs font-normal text-zinc-400">
                  {detail.subtasks.filter((s) => s.completed_at).length}/{detail.subtasks.length}
                </span>
              {/if}
            </h3>
            <ul class="mt-1">
              {#each detail.subtasks as sub (sub.id)}
                <li class="flex items-center gap-2.5 border-b border-zinc-100 py-2 dark:border-zinc-800/70">
                  <button
                    aria-label="Toggle subtask"
                    onclick={() => completeTask(sub, !sub.completed_at).then(load)}
                    class="task-check {checkColors[sub.priority]} {sub.completed_at ? 'bg-current/10' : ''}"
                    style="width: 1.05rem; height: 1.05rem; margin-top: 0"
                  >
                    {#if sub.completed_at}<Check size={11} strokeWidth={3} />{/if}
                  </button>
                  <span class="flex-1 text-sm {sub.completed_at ? 'text-zinc-400 line-through' : ''}">{sub.name}</span>
                  {#if sub.due_date}
                    <span class="text-xs text-zinc-400">{dayLabel(sub.due_date)}{sub.due_time ? ` ${fmtTime(sub.due_time)}` : ''}</span>
                  {/if}
                </li>
              {/each}
            </ul>
            <form onsubmit={submitSubtask} class="mt-2 flex items-center gap-2">
              <Plus size={16} class="text-zinc-400" />
              <input
                bind:value={newSubtask}
                placeholder="Add a sub-task"
                class="flex-1 bg-transparent text-sm outline-none placeholder:text-zinc-400"
              />
            </form>
          </div>
        {/if}

        <!-- attachments -->
        <div class="mt-6">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-semibold">Attachments</h3>
            <button
              onclick={() => fileInput.click()}
              class="flex items-center gap-1 text-xs font-medium text-brand-600 hover:underline"
            >
              <Paperclip size={13} /> Attach file
            </button>
            <input bind:this={fileInput} type="file" class="hidden" onchange={uploadFile} />
          </div>
          {#if detail.attachments.length}
            <ul class="mt-1 space-y-1">
              {#each detail.attachments as att (att.id)}
                <li class="flex items-center gap-2 rounded-lg border border-zinc-100 px-2.5 py-1.5 text-sm dark:border-zinc-800">
                  <Paperclip size={14} class="flex-none text-zinc-400" />
                  <span class="min-w-0 flex-1 truncate">{att.filename}</span>
                  <span class="text-xs text-zinc-400">{fmtSize(att.size)}</span>
                  <a href="/api/attachments/{att.id}" download aria-label="Download" class="p-1 text-zinc-400 hover:text-brand-600">
                    <Download size={14} />
                  </a>
                  <button aria-label="Delete attachment" onclick={() => deleteAttachment(att.id)} class="p-1 text-zinc-400 hover:text-red-600">
                    <Trash2 size={14} />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <!-- comments -->
        <div class="mt-6 pb-2">
          <h3 class="text-sm font-semibold">Comments</h3>
          <ul class="mt-1 space-y-3">
            {#each detail.comments as c (c.id)}
              <li class="flex gap-2.5">
                <div class="flex h-7 w-7 flex-none items-center justify-center rounded-full bg-zinc-100 text-xs font-bold text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
                  {c.user_name.slice(0, 1).toUpperCase()}
                </div>
                <div class="min-w-0 flex-1">
                  <div class="flex items-baseline gap-2">
                    <span class="text-xs font-semibold">{c.user_name}</span>
                    <span class="text-[11px] text-zinc-400">{fmtTimestamp(c.created_at)}</span>
                    {#if c.user_id === data.user.id}
                      <button
                        onclick={() => deleteComment(c.id)}
                        class="ml-auto text-[11px] text-zinc-400 hover:text-red-600"
                      >
                        Delete
                      </button>
                    {/if}
                  </div>
                  <p class="text-sm whitespace-pre-wrap">{c.body}</p>
                </div>
              </li>
            {/each}
          </ul>
        </div>
      </div>

      <!-- comment composer -->
      <form
        onsubmit={submitComment}
        class="flex items-center gap-2 border-t border-zinc-100 px-4 py-3 dark:border-zinc-800"
        style="padding-bottom: max(0.75rem, env(safe-area-inset-bottom))"
      >
        <input
          bind:value={newComment}
          placeholder="Write a comment…"
          class="flex-1 rounded-full border border-zinc-200 bg-transparent px-3.5 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
        />
        <button
          type="submit"
          aria-label="Send comment"
          disabled={!newComment.trim()}
          class="flex h-9 w-9 items-center justify-center rounded-full bg-brand-600 text-white disabled:opacity-40"
        >
          <Send size={15} />
        </button>
      </form>
    {/if}
  </div>
</div>

{#if task && pickerFor === 'due'}
  <DatePicker
    date={task.due_date}
    time={task.due_time}
    onselect={(due_date, due_time) => save({ due_date, due_time })}
    onclose={() => (pickerFor = null)}
  />
{:else if task && pickerFor === 'deadline'}
  <DatePicker
    title="Deadline"
    date={task.deadline}
    allowsTime={false}
    onselect={(deadline) => save({ deadline })}
    onclose={() => (pickerFor = null)}
  />
{/if}
