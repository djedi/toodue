<script>
  import { ui, setColorScheme } from '../state.svelte.js';

  const schemes = [
    { value: 'sky', label: 'Sky', color: '#38bdf8' },
    { value: 'coral', label: 'Coral', color: '#dd4b33' },
    { value: 'emerald', label: 'Emerald', color: '#10b981' },
    { value: 'violet', label: 'Violet', color: '#8b5cf6' },
    { value: 'amber', label: 'Amber', color: '#f59e0b' },
    { value: 'rose', label: 'Rose', color: '#f43f5e' }
  ];
</script>

<div class="flex flex-wrap items-center justify-end gap-1.5" role="group" aria-label="Color scheme picker">
  {#each schemes as scheme (scheme.value)}
    <button
      type="button"
      aria-label="{scheme.label} color scheme"
      title="{scheme.label}"
      onclick={() => setColorScheme(scheme.value)}
      class="flex items-center gap-1.5 rounded-full border px-2 py-1 text-xs font-medium transition {ui.colorScheme ===
      scheme.value
        ? 'border-brand-600 bg-brand-50 text-brand-700 dark:border-brand-400 dark:bg-brand-950 dark:text-brand-200'
        : 'border-zinc-200 text-zinc-500 hover:border-zinc-300 hover:text-zinc-700 dark:border-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200'}"
    >
      <span
        class="h-3 w-3 rounded-full border border-black/10 shadow-sm dark:border-white/20"
        style={`background: ${scheme.color}`}
        aria-hidden="true"
      ></span>
      <span>{scheme.label}</span>
    </button>
  {/each}
</div>
