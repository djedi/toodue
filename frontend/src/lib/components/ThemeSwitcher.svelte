<script>
  import { ui, setTheme } from '../state.svelte.js';
  import { Sun, Monitor, Moon } from '@lucide/svelte';

  let { compact = false } = $props();

  const options = [
    { value: 'light', icon: Sun, label: 'Light' },
    { value: 'system', icon: Monitor, label: 'System' },
    { value: 'dark', icon: Moon, label: 'Dark' }
  ];
</script>

<div
  class="flex items-center rounded-lg border border-zinc-200 bg-white p-0.5 dark:border-zinc-700 dark:bg-zinc-900"
>
  {#each options as opt (opt.value)}
    <button
      aria-label="{opt.label} theme"
      title="{opt.label} theme"
      onclick={() => setTheme(opt.value)}
      class="flex items-center gap-1.5 rounded-md px-1.5 py-1 text-xs font-medium transition {ui.theme ===
      opt.value
        ? 'bg-zinc-200 text-zinc-900 dark:bg-zinc-700 dark:text-zinc-100'
        : 'text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300'}"
    >
      <opt.icon size={14} />
      {#if !compact}<span>{opt.label}</span>{/if}
    </button>
  {/each}
</div>
