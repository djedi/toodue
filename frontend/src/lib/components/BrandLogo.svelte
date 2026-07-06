<script>
  let { size = 'md', showWordmark = true, class: className = '' } = $props();

  const sizeClasses = {
    sm: 'h-8 w-8',
    md: 'h-12 w-12',
    lg: 'h-16 w-16'
  };
</script>

<div class="inline-flex items-center gap-2 {className}">
  <img
    src="/icons/toodue-logo.svg"
    alt={showWordmark ? '' : 'TooDue'}
    class="{sizeClasses[size] ?? sizeClasses.md} shrink-0 object-contain"
    aria-hidden={showWordmark ? 'true' : undefined}
  />
  {#if showWordmark}
    <span class="font-bold tracking-tight">TooDue</span>
  {/if}
</div>
