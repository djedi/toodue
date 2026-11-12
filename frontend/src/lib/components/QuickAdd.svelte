<script>
  import { data, ui, addTask, inboxProject, toast } from '../state.svelte.js';
  import { tick } from 'svelte';
  import { activeShortcut, shortcutOptions, consumeShortcut, parseQuickAdd } from '../quickAdd.js';
  import { dayLabel, fmtTime } from '../dates.js';
  import { normalizeRepeatRule, repeatOptions } from '../recurrence.js';
  import DatePicker from './DatePicker.svelte';
  import { ArrowUp, CalendarDays, ChevronDown, Hash, Inbox, Flag, Repeat2, X } from '@lucide/svelte';

  let name = $state('');
  let description = $state('');
  let due_date = $state(ui.quickAdd?.due_date ?? '');
  let due_time = $state('');
  let deadline = $state('');
  let repeat_rule = $state('');
  let priority = $state(4);
  let project_id = $state(
    ui.quickAdd?.project_id ??
      (ui.view === 'project' ? ui.projectId : null) ??
      inboxProject()?.id
  );
  let busy = $state(false);
  let nameInput = $state(null);
  let cursor = $state(0);
  let dismissed = $state(false);
  let activeIndex = $state(0);
  const token = $derived(dismissed ? null : activeShortcut(name, cursor));
  const suggestions = $derived(shortcutOptions(token, data.projects));
  const parsed = $derived(parseQuickAdd(name));

  function updateCursor(e) {
    cursor = e.currentTarget.selectionStart ?? name.length;
    dismissed = false;
    activeIndex = 0;
  }

  async function choose(option) {
    if (!token || !option) return;
    const result = consumeShortcut(name, token);
    name = result.name;
    if ('project_id' in option) project_id = option.project_id;
    if ('priority' in option) priority = option.priority;
    if ('due_date' in option) {
      due_date = option.repeat_rule && due_date ? due_date : option.due_date ?? '';
      if (!due_date) { due_time = ''; repeat_rule = ''; }
    }
    if (option.repeat_rule) repeat_rule = option.repeat_rule;
    dismissed = true;
    await tick();
    nameInput?.focus();
    nameInput?.setSelectionRange(result.cursor, result.cursor);
    cursor = result.cursor;
  }

  function inputKeydown(e) {
    if (e.isComposing) return;
    if (e.key === 'Escape' && token) {
      e.preventDefault(); e.stopPropagation(); dismissed = true; return;
    }
    if (!suggestions.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      activeIndex = (activeIndex + (e.key === 'ArrowDown' ? 1 : -1) + suggestions.length) % suggestions.length;
      document.getElementById(`quick-add-option-${activeIndex}`)?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter' || (e.key === 'Tab' && !e.shiftKey)) {
      e.preventDefault(); choose(suggestions[activeIndex]);
    }
  }

  let pickerFor = $state(null); // 'due' | 'deadline'

  $effect(() => {
    nameInput?.focus();
  });

  const selectable = $derived(data.projects);
  const selectedProject = $derived(selectable.find(p => p.id === project_id));
  const effectivePriority = $derived(parsed.priority ?? Number(priority));

  function keepInputFocus(e) {
    // Mouse selection keeps the caret; touch must remain free to scroll.
    if (e.pointerType === 'mouse') e.preventDefault();
  }

  function close() {
    ui.quickAdd = null;
  }

  async function submit(e) {
    e?.preventDefault();
    const task = parseQuickAdd(name);
    if (!task.name || busy) return;
    busy = true;
    // addTask inserts the row optimistically, so the dialog can close right away;
    // a hard failure is reported by toast after the fact.
    const pending = addTask({
      project_id,
      name: task.name,
      description,
      due_date: due_date || null,
      due_time: due_date && due_time ? due_time : null,
      deadline: deadline || null,
      repeat_rule: normalizeRepeatRule(repeat_rule, due_date),
      priority: task.priority ?? Number(priority)
    });
    close();
    try {
      await pending;
    } catch (err) {
      toast(err.message);
    }
  }

  function onkeydown(e) {
    if (e.key === 'Escape' && !pickerFor) close();
  }

  const chip =
    'metadata-chip';
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-40 flex items-end justify-center bg-black/40 sm:items-start sm:pt-[18vh]"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && close()}
>
  <form
    onsubmit={submit}
    class="composer relative max-h-[85dvh] overflow-y-auto w-full rounded-t-2xl bg-white shadow-xl sm:max-w-xl sm:rounded-2xl dark:bg-zinc-900"
    style="padding-bottom: max(1rem, env(safe-area-inset-bottom))"
  >
    <div class="composer-heading">
      <span class="composer-eyebrow">New task</span>
      <button type="button" aria-label="Close" onclick={close} class="close-button"><X size={18} /></button>
    </div>
    <div class="task-entry">
      <input
        id="quick-add-name"
        role="combobox"
        aria-label="Task name"
        aria-autocomplete="list"
        aria-expanded={suggestions.length > 0}
        aria-controls={suggestions.length ? 'quick-add-suggestions' : undefined}
        aria-activedescendant={suggestions.length ? `quick-add-option-${activeIndex}` : undefined}
        aria-describedby="quick-add-hint"
        autocomplete="off"
        oninput={updateCursor}
        onclick={updateCursor}
        onkeyup={(e) => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) updateCursor(e); }}
        onkeydown={inputKeydown}
        bind:this={nameInput}
        bind:value={name}
        placeholder="What needs to get done?"
        class="task-name"
      />
    </div>
    {#if suggestions.length}
      <div id="quick-add-suggestions" role="listbox" aria-label="Task shortcuts" class="suggestions">
        <div class="suggestion-heading">{token?.kind === '#' ? 'Projects' : token?.kind === '!!' ? 'Priority' : 'Dates & repeats'}</div>
        {#each suggestions as option, index}
          <button type="button" role="option" tabindex="-1" id={`quick-add-option-${index}`} aria-selected={activeIndex === index}
            class="suggestion" onpointerdown={keepInputFocus} onclick={() => choose(option)}>
            <span class="suggestion-icon" data-priority={option.priority}>
              {#if token?.kind === '#'}
                <Hash size={18} style={`color: ${data.projects.find(p => p.id === option.project_id)?.color || 'currentColor'}`} />
              {:else if option.priority}<Flag size={18} fill={option.priority < 4 ? 'currentColor' : 'none'} />
              {:else if option.repeat_rule}<Repeat2 size={18} />
              {:else}<CalendarDays size={18} />{/if}
            </span>
            <span class="suggestion-label">{option.label}</span>
            {#if activeIndex === index}<span class="suggestion-return" aria-hidden="true">↵</span>{/if}
          </button>
        {/each}
      </div>
    {/if}
    <details class="composer-details">
      <summary>Description & details <ChevronDown size={14} /></summary>
    <textarea
      id="quick-add-description"
      aria-label="Description"
      bind:value={description}
      placeholder="Description"
      rows="2"
      class="mt-1 w-full resize-none bg-transparent text-sm outline-none placeholder:text-zinc-400"
    ></textarea>

    <div class="mt-2 flex flex-wrap items-center gap-2">
      <label class={chip} title="Repeat">
        <Repeat2 size={14} class="text-brand-600" />
        <select
          id="quick-add-repeat"
          aria-label="Repeat"
          bind:value={repeat_rule}
          disabled={!due_date}
          class="bg-transparent outline-none disabled:opacity-50"
        >
          {#each repeatOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <button type="button" class={chip} title="Deadline" onclick={() => (pickerFor = 'deadline')}>
        <Flag size={14} class="text-amber-600" />
        {deadline ? dayLabel(deadline) : 'Deadline'}
      </button>
      <select
        id="quick-add-priority"
        aria-label="Priority"
        bind:value={priority}
        class="{chip} bg-transparent outline-none"
        title="Priority"
      >
        <option value={1}>Priority 1</option>
        <option value={2}>Priority 2</option>
        <option value={3}>Priority 3</option>
        <option value={4}>Priority 4</option>
      </select>
    </div>

    </details>
    <div class="composer-help">
      <p id="quick-add-hint"><span>#</span> project <span>/</span> date <span>!!</span> priority</p>
      <details><summary>Shortcuts</summary><p id="quick-add-help">Type # to find a project, / for dates or repeats, and !!1–!!4 for priority. Choose with ↑ ↓ and Enter or Tab. Escape keeps your text. Project names can include spaces or quotes; only confirmed shortcuts are removed.</p></details>
    </div>
    <div class="composer-footer">
      <button type="button" class={chip} onclick={() => (pickerFor = 'due')}>
        <CalendarDays size={14} class="text-emerald-600" />
        {due_date ? `${dayLabel(due_date)}${due_time ? ` · ${fmtTime(due_time)}` : ''}` : 'Date'}
      </button>
      {#if priority !== 4 || parsed.priority}
        <span class={chip} data-priority={effectivePriority}><Flag size={14} fill="currentColor" /> P{effectivePriority}</span>
      {/if}
      <label class="project-chip metadata-chip">
        {#if selectedProject?.is_inbox}<Inbox size={16} />{:else}<Hash size={16} style={`color: ${selectedProject?.color || 'currentColor'}`} />{/if}
      <select
        aria-label="Project"
        id="quick-add-project"
        bind:value={project_id}
        class="project-select"
      >
        {#each selectable as p (p.id)}
          <option value={p.id}>{p.is_inbox ? 'Inbox' : p.name}</option>
        {/each}
      </select>
      </label>
      <button
        type="submit"
        aria-label="Add task"
        title="Add task"
        disabled={!parsed.name || busy}
        class="add-button"
      >
        <ArrowUp size={22} strokeWidth={2.5} />
      </button>
    </div>
  </form>
</div>

{#if pickerFor === 'due'}
  <DatePicker
    date={due_date || null}
    time={due_time || null}
    onselect={(d, t) => {
      due_date = d ?? '';
      due_time = t ?? '';
    }}
    onclose={() => (pickerFor = null)}
  />
{:else if pickerFor === 'deadline'}
  <DatePicker
    title="Deadline"
    date={deadline || null}
    allowsTime={false}
    onselect={(d) => (deadline = d ?? '')}
    onclose={() => (pickerFor = null)}
  />
{/if}

<style>
  /* A quiet writing surface, with metadata collected along a single action rail.
     Use the app's system type and theme tokens rather than introducing a second UI. */
  .composer { --composer-muted: var(--color-zinc-500); --composer-line: var(--color-zinc-200); --composer-soft: var(--color-zinc-50); --composer-ink: var(--color-zinc-900); padding: 1.25rem; color: var(--composer-ink); }
  :global(.dark) .composer { --composer-muted: var(--color-zinc-400); --composer-line: var(--color-zinc-700); --composer-soft: var(--color-zinc-800); --composer-ink: var(--color-zinc-100); }
  .composer-heading { display:flex; align-items:center; justify-content:space-between; gap:1rem; }
  .composer-eyebrow { font-size:.6875rem; font-weight:650; letter-spacing:.09em; text-transform:uppercase; color:var(--composer-muted); }
  .close-button { display:grid; place-items:center; width:2.75rem; height:2.75rem; margin:-.5rem -.5rem -.25rem 0; border-radius:50%; color:var(--composer-muted); }
  .close-button:hover { background:var(--composer-soft); }
  .task-entry { padding-block:.5rem .75rem; }
  .task-name { width:100%; min-width:0; min-height:3rem; background:transparent; font-weight:600; letter-spacing:-.025em; font-size:1.375rem !important; line-height:1.5; outline:none; border-radius:.25rem; }
  .task-name::placeholder { color:var(--composer-muted); font-weight:450; }
  .task-name:focus-visible { box-shadow:0 2px 0 var(--color-brand-500); }
  .composer-details { color:var(--composer-muted); font-size:.8125rem; }
  summary { cursor:pointer; min-height:2.75rem; display:flex; align-items:center; gap:.5rem; list-style:none; }
  summary::-webkit-details-marker { display:none; }
  .composer-details[open] summary :global(svg) { transform:rotate(180deg); }
  .composer-help { display:flex; align-items:center; justify-content:space-between; gap:.75rem; padding-block:.25rem .75rem; color:var(--composer-muted); font-size:.6875rem; }
  .composer-help > p { display:flex; align-items:center; gap:.3rem; flex-wrap:wrap; }
  .composer-help > p span { font-weight:700; color:var(--composer-ink); }
  .composer-help details { flex:none; }
  .composer-help details[open] { flex:1; }
  .composer-help:has(details[open]) { flex-wrap:wrap; }
  .composer-help details p { max-width:55ch; line-height:1.7; }
  .composer-help summary { min-height:2rem; text-decoration:underline; text-underline-offset:3px; }
  .composer-footer { display:flex; flex-wrap:wrap; align-items:center; gap:.5rem; border-top:1px solid var(--composer-line); padding-top:1rem; }
  .composer :global(.metadata-chip) { display:inline-flex; align-items:center; gap:.375rem; min-height:2.75rem; max-width:100%; border:1px solid var(--composer-line); border-radius:.625rem; padding:.375rem .625rem; font-size:.8125rem; color:var(--composer-muted); background:transparent; }
  .composer :global(.metadata-chip svg) { flex:none; }
  .composer :global(.metadata-chip select) { min-width:0; max-width:100%; }
  .project-chip { flex:1 1 8rem; min-width:min(100%, 8rem); max-width:16rem !important; }
  .project-select { width:100%; min-width:0; background:transparent; outline:none; text-overflow:ellipsis; }
  select option { background:var(--composer-soft); color:var(--composer-ink); }
  .add-button { display:grid; place-items:center; flex:none; width:2.75rem; height:2.75rem; margin-left:auto; border-radius:.75rem; background:var(--color-brand-600); color:white; }
  .add-button:hover { background:var(--color-brand-700); }
  .add-button:disabled { opacity:.35; }
  button:focus-visible, summary:focus-visible, select:focus-visible { outline:2px solid var(--color-brand-500); outline-offset:3px; }
  .suggestions { max-height:15rem; overflow-y:auto; overscroll-behavior:contain; border:1px solid var(--composer-line); border-radius:.875rem; padding:.25rem; margin-bottom:.5rem; background:var(--composer-soft); }
  .suggestion-heading { padding:.5rem .75rem; font-size:.6875rem; font-weight:600; color:var(--composer-muted); }
  .suggestion { display:flex; align-items:center; gap:.75rem; width:100%; min-height:2.875rem; border-radius:.625rem; padding:.625rem .75rem; text-align:left; font-size:.875rem; }
  .suggestion[aria-selected='true'] { background:var(--color-brand-100); color:var(--color-brand-900); }
  :global(.dark) .suggestion[aria-selected='true'] { background:var(--color-brand-950); color:var(--color-brand-200); }
  .suggestion-icon { display:flex; flex:none; color:var(--composer-muted); }
  .suggestion-label { flex:1; min-width:0; overflow-wrap:anywhere; }
  .suggestion-return { color:var(--composer-muted); }
  .composer :global([data-priority='1']) { color:var(--color-red-500); }
  .composer :global([data-priority='2']) { color:var(--color-amber-500); }
  .composer :global([data-priority='3']) { color:var(--color-blue-500); }
</style>
