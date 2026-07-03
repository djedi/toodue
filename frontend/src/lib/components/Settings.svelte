<script>
  import { api } from '../api.js';
  import { data, ui, refresh, signOut, toast } from '../state.svelte.js';
  import ThemeSwitcher from './ThemeSwitcher.svelte';
  import { X, CalendarPlus, Copy, FileUp, ChevronRight, LogOut } from '@lucide/svelte';

  let importInput = $state(null);
  let importing = $state(false);

  function close() {
    ui.showSettings = false;
  }

  function onkeydown(e) {
    if (e.key === 'Escape') close();
  }

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

  const row =
    'flex w-full items-center gap-3 rounded-xl border border-zinc-200 px-4 py-3 text-left dark:border-zinc-800';
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-40 flex items-end justify-center bg-black/40 sm:items-center sm:p-6"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && close()}
>
  <div
    class="w-full rounded-t-2xl bg-white p-5 shadow-xl sm:max-w-md sm:rounded-2xl dark:bg-zinc-900"
    style="padding-bottom: max(1.25rem, env(safe-area-inset-bottom))"
  >
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold">Settings</h2>
      <button aria-label="Close" onclick={close} class="p-1 text-zinc-400 hover:text-zinc-600">
        <X size={18} />
      </button>
    </div>

    <div class="mt-3 flex items-center gap-3">
      <div
        class="flex h-10 w-10 flex-none items-center justify-center rounded-full bg-brand-100 text-base font-bold text-brand-700 dark:bg-brand-950 dark:text-brand-300"
      >
        {data.user.name.slice(0, 1).toUpperCase()}
      </div>
      <div class="min-w-0">
        <p class="truncate text-sm font-semibold">{data.user.name}</p>
        <p class="truncate text-xs text-zinc-400">{data.user.email}</p>
      </div>
    </div>

    <div class="mt-4 space-y-3">
      <div class="{row} justify-between">
        <span class="text-sm font-medium">Theme</span>
        <ThemeSwitcher />
      </div>

      <button onclick={copyCalendarUrl} class={row}>
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
        class="{row} disabled:opacity-50"
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
      <input
        bind:this={importInput}
        type="file"
        accept=".zip"
        class="hidden"
        onchange={importTodoist}
      />

      <button
        onclick={signOut}
        class="{row} text-sm font-medium text-red-600 dark:text-red-400"
      >
        <LogOut size={18} /> Log out
      </button>
    </div>
  </div>
</div>
