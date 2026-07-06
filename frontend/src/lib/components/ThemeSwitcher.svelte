<script>
  import { ui, setTheme } from '../state.svelte.js';

  let { compact = false } = $props();

  // Font Awesome Free solid icons: faSun, faMoon, faDesktop.
  const options = [
    {
      value: 'light',
      label: 'Light',
      path: 'M256 160c-52.9 0-96 43.1-96 96s43.1 96 96 96 96-43.1 96-96-43.1-96-96-96zm246.4 80.5-94.7-47.3 33.5-100.4c4.5-13.5-8.4-26.4-21.9-21.9l-100.4 33.5-47.4-94.8c-6.4-12.8-24.6-12.8-31 0l-47.3 94.7L92.7 70.8c-13.5-4.5-26.4 8.4-21.9 21.9l33.5 100.4-94.7 47.4c-12.8 6.4-12.8 24.6 0 31l94.7 47.3-33.5 100.5c-4.5 13.5 8.4 26.4 21.9 21.9l100.4-33.5 47.3 94.7c6.4 12.8 24.6 12.8 31 0l47.3-94.7 100.4 33.5c13.5 4.5 26.4-8.4 21.9-21.9l-33.5-100.4 94.7-47.3c13-6.5 13-24.7.2-31.1z'
    },
    {
      value: 'dark',
      label: 'Dark',
      path: 'M283.2 512c79 0 151.1-35.9 198.9-94.8 7.1-8.7-.6-21.4-11.6-19.4-124.2 23.7-238.3-71.6-238.3-197 0-72.2 38.7-138.6 101.5-174.4 9.7-5.5 7.3-20.2-3.8-22.2C314.7 1.4 299.2 0 283.2 0 141.3 0 26.2 114.5 26.2 256S141.3 512 283.2 512z'
    },
    {
      value: 'system',
      label: 'System',
      path: 'M480 0H32C14.3 0 0 14.3 0 32v320c0 17.7 14.3 32 32 32h192v64h-73.6c-14.4 0-26.8 9.6-30.4 23.5-1.7 6.7 3.3 13.2 10.2 13.2h251.6c6.9 0 11.9-6.5 10.2-13.2-3.6-13.9-16-23.5-30.4-23.5H288v-64h192c17.7 0 32-14.3 32-32V32c0-17.7-14.3-32-32-32zm-32 320H64V64h384v256z'
    }
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
      <svg
        class="h-3.5 w-3.5 fill-current"
        aria-hidden="true"
        focusable="false"
        viewBox="0 0 512 512"
      >
        <path d={opt.path} />
      </svg>
      {#if !compact}<span>{opt.label}</span>{/if}
    </button>
  {/each}
</div>
