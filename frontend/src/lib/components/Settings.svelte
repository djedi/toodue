<script>
  import { api } from '../api.js';
  import { apiKeyCreatedMessage, maskApiKey } from '../apiKeys.js';
  import { data, ui, refresh, signOut, toast } from '../state.svelte.js';
  import ThemeSwitcher from './ThemeSwitcher.svelte';
  import ColorSchemePicker from './ColorSchemePicker.svelte';
  import {
    X,
    CalendarPlus,
    CalendarSync,
    Copy,
    FileUp,
    ChevronRight,
    KeyRound,
    LogOut,
    Trash2
  } from '@lucide/svelte';

  let importInput = $state(null);
  let importing = $state(false);
  let google = $state(null); // { configured, connected }
  let apiKeys = $state([]);
  let apiKeyName = $state('');
  let newApiKey = $state(null);
  let apiKeyBusy = $state(false);

  $effect(() => {
    api.get('/google/status').then((g) => (google = g)).catch(() => {});
    api.get('/api-keys').then((keys) => (apiKeys = keys)).catch(() => {});
  });

  async function createApiKey(e) {
    e.preventDefault();
    const name = apiKeyName.trim();
    if (!name || apiKeyBusy) return;
    apiKeyBusy = true;
    try {
      const created = await api.post('/api-keys', { name });
      apiKeys = [created.api_key, ...apiKeys];
      newApiKey = created.key;
      apiKeyName = '';
      await navigator.clipboard.writeText(created.key).catch(() => {});
      toast(apiKeyCreatedMessage(name));
    } catch (err) {
      toast(err.message);
    } finally {
      apiKeyBusy = false;
    }
  }

  async function revokeApiKey(id) {
    if (!confirm('Revoke this API key? Apps using it will stop working immediately.')) return;
    try {
      await api.del(`/api-keys/${id}`);
      apiKeys = apiKeys.filter((k) => k.id !== id);
      toast('API key revoked');
    } catch (err) {
      toast(err.message);
    }
  }

  async function copyNewApiKey() {
    if (!newApiKey) return;
    await navigator.clipboard.writeText(newApiKey);
    toast('API key copied');
  }

  async function disconnectGoogle() {
    if (!confirm('Disconnect Google Calendar? The TooDue calendar will be removed from your Google account.'))
      return;
    try {
      await api.post('/google/disconnect');
      google = { ...google, connected: false };
      toast('Google Calendar disconnected');
    } catch (err) {
      toast(err.message);
    }
  }

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

      <div class="{row} flex-col items-stretch gap-2">
        <div>
          <span class="block text-sm font-medium">Color scheme</span>
          <span class="block text-xs text-zinc-400">Pick the main accent color for buttons, links, and highlights</span>
        </div>
        <ColorSchemePicker />
      </div>

      {#if google?.configured}
        {#if google.connected}
          <div class={row}>
            <CalendarSync size={18} class="text-emerald-600" />
            <span class="flex-1">
              <span class="block text-sm font-medium">Google Calendar connected</span>
              <span class="block text-xs text-zinc-400">
                Dated tasks sync both ways with your "TooDue" calendar
              </span>
            </span>
            <button onclick={disconnectGoogle} class="text-xs font-medium text-red-600 hover:underline dark:text-red-400">
              Disconnect
            </button>
          </div>
        {:else}
          <a href="/api/google/connect" class={row}>
            <CalendarSync size={18} class="text-zinc-400" />
            <span class="flex-1">
              <span class="block text-sm font-medium">Connect Google Calendar</span>
              <span class="block text-xs text-zinc-400">
                Two-way sync — move an event in Google and the task's date follows
              </span>
            </span>
            <ChevronRight size={15} class="text-zinc-400" />
          </a>
        {/if}
      {/if}

      <button onclick={copyCalendarUrl} class={row}>
        <CalendarPlus size={18} class="text-zinc-400" />
        <span class="flex-1">
          <span class="block text-sm font-medium">Calendar feed (read-only)</span>
          <span class="block text-xs text-zinc-400">
            Copy an iCal URL for Fantastical or other calendar apps
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

      <div class="rounded-xl border border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <div class="flex items-start gap-3">
          <KeyRound size={18} class="mt-0.5 flex-none text-zinc-400" />
          <div class="min-w-0 flex-1">
            <span class="block text-sm font-medium">API keys</span>
            <span class="block text-xs text-zinc-400">Use with AI agents, scripts, and the TooDue MCP server</span>
          </div>
        </div>

        {#if newApiKey}
          <div class="mt-3 rounded-lg border border-brand-200 bg-brand-50 p-2 text-xs text-brand-950 dark:border-brand-950 dark:bg-brand-950/60 dark:text-brand-100">
            <div class="font-semibold">Copy this key now. It will only be shown once.</div>
            <button type="button" onclick={copyNewApiKey} class="mt-1 w-full truncate rounded bg-white/70 px-2 py-1 text-left font-mono dark:bg-zinc-900/60">
              {newApiKey}
            </button>
          </div>
        {/if}

        <form onsubmit={createApiKey} class="mt-3 flex gap-2">
          <input
            bind:value={apiKeyName}
            placeholder="Key name, e.g. Claude Desktop"
            class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
          />
          <button
            type="submit"
            disabled={!apiKeyName.trim() || apiKeyBusy}
            class="rounded-lg bg-brand-600 px-3 py-2 text-sm font-semibold text-white disabled:opacity-40"
          >
            Create
          </button>
        </form>

        {#if apiKeys.length}
          <ul class="mt-3 space-y-1">
            {#each apiKeys as key (key.id)}
              <li class="flex items-center gap-2 rounded-lg py-1.5">
                <div class="min-w-0 flex-1">
                  <div class="truncate text-sm font-medium">{key.name}</div>
                  <div class="truncate font-mono text-xs text-zinc-400">
                    {maskApiKey(key.prefix)} · created {key.created_at.slice(0, 10)}{key.last_used_at ? ` · used ${key.last_used_at.slice(0, 10)}` : ''}
                  </div>
                </div>
                <button type="button" aria-label="Revoke API key" onclick={() => revokeApiKey(key.id)} class="p-1 text-zinc-400 hover:text-red-600">
                  <Trash2 size={15} />
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <button
        onclick={signOut}
        class="{row} text-sm font-medium text-red-600 dark:text-red-400"
      >
        <LogOut size={18} /> Log out
      </button>
    </div>
  </div>
</div>
