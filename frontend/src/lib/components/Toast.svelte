<script>
  import { ui } from '../state.svelte.js';
</script>

{#if ui.toast}
  <div
    class="fixed bottom-24 left-1/2 z-50 -translate-x-1/2 rounded-lg bg-zinc-900 px-4 py-2.5 text-sm text-white shadow-lg md:bottom-8 dark:bg-zinc-100 dark:text-zinc-900"
  >
    {ui.toast}
  </div>
{/if}
