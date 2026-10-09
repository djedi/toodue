<script>
  import { api } from '../api.js';
  import { data, signOut } from '../state.svelte.js';
  import { SCOPES, isAppRedirect } from '../oauth.js';
  import BrandLogo from './BrandLogo.svelte';
  import { Bot, Check, ExternalLink, ShieldCheck, TriangleAlert } from '@lucide/svelte';

  let { requestId } = $props();

  let info = $state(null); // { client_name, client_uri, redirect, scopes, connected_scopes }
  let error = $state('');
  let chosen = $state([]);
  let busy = $state(false);
  let done = $state(null); // 'approved' | 'denied'
  let appRedirect = $state(false);

  $effect(() => {
    if (!requestId) {
      error = 'This link is missing its connection request. Start again from your AI agent.';
      return;
    }
    api
      .get(`/oauth/requests/${encodeURIComponent(requestId)}`)
      .then((r) => {
        info = r;
        chosen = [...r.scopes];
      })
      .catch((err) => (error = err.message));
  });

  const requested = $derived(SCOPES.filter((s) => info?.scopes.includes(s.id)));

  function toggle(id) {
    chosen = chosen.includes(id) ? chosen.filter((s) => s !== id) : [...chosen, id];
  }

  async function decide(approve) {
    if (busy) return;
    busy = true;
    try {
      const { redirect_to } = await api.post(`/oauth/requests/${encodeURIComponent(requestId)}`, {
        approve,
        scopes: chosen
      });
      done = approve ? 'approved' : 'denied';
      appRedirect = isAppRedirect(redirect_to);
      history.replaceState(null, '', '/');
      window.location.href = redirect_to;
    } catch (err) {
      error = err.message;
    } finally {
      busy = false;
    }
  }

  function backToApp() {
    history.replaceState(null, '', '/');
    location.reload();
  }
</script>

<div class="flex min-h-dvh items-center justify-center bg-zinc-50 px-4 py-10 dark:bg-zinc-950">
  <div class="w-full max-w-md">
    <div class="mb-6 flex justify-center">
      <BrandLogo size="md" class="text-2xl" />
    </div>

    <div class="rounded-2xl border border-zinc-200 bg-white p-6 shadow-sm dark:border-zinc-800 dark:bg-zinc-900">
      {#if done}
        <div class="text-center">
          <div
            class="mx-auto flex h-12 w-12 items-center justify-center rounded-full {done === 'approved'
              ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300'
              : 'bg-zinc-100 text-zinc-500 dark:bg-zinc-800'}"
          >
            <Check size={24} />
          </div>
          <h1 class="mt-3 text-lg font-semibold">
            {done === 'approved' ? 'Connected' : 'Access denied'}
          </h1>
          <p class="mt-1 text-sm text-zinc-500">
            {#if appRedirect}
              Your browser should hand you back to {info.client_name}. You can close this tab.
            {:else}
              Returning you to {info.client_name}…
            {/if}
          </p>
          <button onclick={backToApp} class="mt-4 text-sm font-medium text-brand-600 hover:underline">
            Go to TooDue
          </button>
        </div>
      {:else if error}
        <div class="flex items-start gap-3">
          <TriangleAlert size={20} class="mt-0.5 flex-none text-amber-500" />
          <div>
            <h1 class="font-semibold">Can't connect</h1>
            <p class="mt-1 text-sm text-zinc-500">{error}</p>
            <button onclick={backToApp} class="mt-4 text-sm font-medium text-brand-600 hover:underline">
              Go to TooDue
            </button>
          </div>
        </div>
      {:else if !info}
        <p class="py-8 text-center text-sm text-zinc-400">Loading…</p>
      {:else}
        <div class="flex items-center gap-3">
          <div
            class="flex h-11 w-11 flex-none items-center justify-center rounded-xl bg-brand-100 text-brand-700 dark:bg-brand-950 dark:text-brand-300"
          >
            <Bot size={22} />
          </div>
          <div class="min-w-0">
            <h1 class="text-base leading-snug font-semibold">
              <span class="break-words">{info.client_name}</span> wants to access your TooDue account
            </h1>
            {#if info.client_uri}
              <a
                href={info.client_uri}
                target="_blank"
                rel="noopener noreferrer"
                class="inline-flex items-center gap-1 text-xs text-zinc-400 hover:underline"
              >
                {new URL(info.client_uri).host}
                <ExternalLink size={11} />
              </a>
            {/if}
          </div>
        </div>

        <div class="mt-4 flex items-center justify-between rounded-lg bg-zinc-50 px-3 py-2 text-xs dark:bg-zinc-800/60">
          <span class="truncate text-zinc-500">
            Signed in as <span class="font-medium text-zinc-700 dark:text-zinc-200">{data.user.email}</span>
          </span>
          <button onclick={signOut} class="ml-2 flex-none font-medium text-brand-600 hover:underline">
            Not you?
          </button>
        </div>

        <p class="mt-5 text-sm font-medium">This agent will be able to:</p>
        <ul class="mt-2 space-y-2">
          {#each requested as scope (scope.id)}
            <li>
              <label
                class="flex items-start gap-3 rounded-xl border border-zinc-200 px-3 py-2.5 dark:border-zinc-800 {scope.required
                  ? ''
                  : 'cursor-pointer'}"
              >
                <input
                  type="checkbox"
                  checked={scope.required || chosen.includes(scope.id)}
                  disabled={scope.required}
                  onchange={() => toggle(scope.id)}
                  class="mt-0.5 h-4 w-4 accent-brand-600"
                />
                <span class="min-w-0">
                  <span class="block text-sm font-medium">
                    {scope.label}
                    {#if scope.required}<span class="ml-1 text-xs font-normal text-zinc-400">required</span>{/if}
                  </span>
                  <span class="block text-xs text-zinc-400">{scope.detail}</span>
                </span>
              </label>
            </li>
          {/each}
        </ul>

        <div class="mt-4 flex items-start gap-2 text-xs text-zinc-500">
          <ShieldCheck size={15} class="mt-0.5 flex-none text-zinc-400" />
          <span>
            The agent acts as you, including in projects shared with you. It never sees your password.
            {#if info.connected_scopes}
              It's already connected; approving replaces its current permissions.
            {/if}
            You can disconnect it any time in Settings → AI agents.
          </span>
        </div>

        <p class="mt-3 text-xs text-zinc-400">
          Only approve if you just started connecting from this agent. After you decide, you'll be sent to
          <span class="font-mono text-zinc-600 dark:text-zinc-300">{info.redirect}</span>.
        </p>

        <div class="mt-5 flex gap-2">
          <button
            onclick={() => decide(false)}
            disabled={busy}
            class="flex-1 rounded-lg border border-zinc-300 py-2.5 text-sm font-semibold disabled:opacity-50 dark:border-zinc-700"
          >
            Deny
          </button>
          <button
            onclick={() => decide(true)}
            disabled={busy}
            class="flex-1 rounded-lg bg-brand-600 py-2.5 text-sm font-semibold text-white transition hover:bg-brand-700 disabled:opacity-50"
          >
            {busy ? 'One moment…' : 'Allow access'}
          </button>
        </div>
      {/if}
    </div>
  </div>
</div>
