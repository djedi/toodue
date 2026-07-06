<script>
  import { api } from '../api.js';
  import { data, afterSignIn } from '../state.svelte.js';
  import BrandLogo from './BrandLogo.svelte';

  let mode = $state('login');
  let name = $state('');
  let email = $state('');
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(e) {
    e.preventDefault();
    error = '';
    busy = true;
    try {
      const path = mode === 'login' ? '/auth/login' : '/auth/register';
      const body = mode === 'login' ? { email, password } : { name, email, password };
      data.user = await api.post(path, body);
      await afterSignIn();
    } catch (err) {
      error = err.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex min-h-dvh items-center justify-center bg-zinc-50 px-4 dark:bg-zinc-950">
  <div class="w-full max-w-sm">
    <div class="mb-8 flex justify-center">
      <BrandLogo size="lg" class="text-3xl" />
    </div>

    <div class="rounded-2xl border border-zinc-200 bg-white p-6 shadow-sm dark:border-zinc-800 dark:bg-zinc-900">
      <h1 class="mb-4 text-lg font-semibold">
        {mode === 'login' ? 'Welcome back' : 'Create your account'}
      </h1>
      <form onsubmit={submit} class="space-y-3">
        {#if mode === 'register'}
          <input
            bind:value={name}
            placeholder="Your name"
            autocomplete="name"
            required
            class="w-full rounded-lg border border-zinc-300 bg-transparent px-3 py-2.5 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20 dark:border-zinc-700"
          />
        {/if}
        <input
          bind:value={email}
          type="email"
          placeholder="Email"
          autocomplete="email"
          required
          class="w-full rounded-lg border border-zinc-300 bg-transparent px-3 py-2.5 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20 dark:border-zinc-700"
        />
        <input
          bind:value={password}
          type="password"
          placeholder="Password"
          autocomplete={mode === 'login' ? 'current-password' : 'new-password'}
          required
          minlength="8"
          class="w-full rounded-lg border border-zinc-300 bg-transparent px-3 py-2.5 text-sm outline-none focus:border-brand-500 focus:ring-2 focus:ring-brand-500/20 dark:border-zinc-700"
        />
        {#if error}
          <p class="text-sm text-red-600 dark:text-red-400">{error}</p>
        {/if}
        <button
          type="submit"
          disabled={busy}
          class="w-full rounded-lg bg-brand-600 py-2.5 text-sm font-semibold text-white transition hover:bg-brand-700 disabled:opacity-50"
        >
          {busy ? 'One moment…' : mode === 'login' ? 'Log in' : 'Sign up'}
        </button>
      </form>
    </div>

    <p class="mt-4 text-center text-sm text-zinc-500">
      {mode === 'login' ? "Don't have an account?" : 'Already have an account?'}
      <button
        class="font-semibold text-brand-600 hover:underline"
        onclick={() => {
          mode = mode === 'login' ? 'register' : 'login';
          error = '';
        }}
      >
        {mode === 'login' ? 'Sign up' : 'Log in'}
      </button>
    </p>

    <p class="mt-3 text-center text-sm text-zinc-500">
      Building an integration?
      <a
        class="font-semibold text-brand-600 hover:underline"
        href="https://docs.toodue.com"
        target="_blank"
        rel="noreferrer"
      >API docs</a>
    </p>
  </div>
</div>
