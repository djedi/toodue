<script>
  import {
    quickOptions,
    monthStarts,
    monthGrid,
    monthLabel,
    parseNatural,
    weekdayAbbrev
  } from '../datePicker.js';
  import { todayStr, dayLabel } from '../dates.js';
  import { X, Check, CalendarDays, Sun, Sofa, ArrowRight, Ban, Clock } from '@lucide/svelte';

  let { title = 'Date', date = null, time = null, allowsTime = true, onselect, onclose } = $props();

  let selectedDay = $state(date);
  let selectedTime = $state(time ?? '');
  let typed = $state('');

  const today = todayStr();
  const suggestion = $derived(parseNatural(typed, today));
  const months = monthStarts(13, today);
  const options = quickOptions(today);

  const optionIcons = {
    today: CalendarDays,
    tomorrow: Sun,
    weekend: Sofa,
    'next-week': ArrowRight,
    'no-date': Ban
  };
  const optionColors = {
    today: 'text-emerald-600',
    tomorrow: 'text-amber-500',
    weekend: 'text-blue-500',
    'next-week': 'text-violet-500',
    'no-date': 'text-zinc-400'
  };

  function apply(day) {
    selectedDay = day;
    if (!day) selectedTime = '';
    confirm();
  }

  function confirm() {
    onselect(selectedDay, selectedDay && selectedTime ? selectedTime : null);
    onclose();
  }

  function onTimeChange(e) {
    selectedTime = e.target.value;
    if (selectedTime && !selectedDay) selectedDay = today;
  }

  function onkeydown(e) {
    if (e.key === 'Escape') onclose();
  }

  function dayClasses(day) {
    if (day === selectedDay) return 'bg-brand-600 font-semibold text-white';
    if (day < today) return 'text-zinc-300 dark:text-zinc-600';
    if (day === today) return 'font-semibold text-brand-600 hover:bg-zinc-100 dark:hover:bg-zinc-800';
    return 'hover:bg-zinc-100 dark:hover:bg-zinc-800';
  }
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-50 flex items-end justify-center bg-black/40 sm:items-center sm:p-6"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div
    class="flex max-h-[88vh] w-full flex-col overflow-hidden rounded-t-2xl bg-white shadow-xl sm:max-h-[80vh] sm:max-w-sm sm:rounded-2xl dark:bg-zinc-900"
    style="padding-bottom: env(safe-area-inset-bottom)"
  >
    <!-- header -->
    <div class="flex items-center justify-between px-4 pt-4 pb-2">
      <button
        aria-label="Close"
        onclick={onclose}
        class="flex h-9 w-9 items-center justify-center rounded-full bg-zinc-100 text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
      >
        <X size={17} />
      </button>
      <h2 class="text-base font-semibold">{title}</h2>
      <button
        aria-label="Confirm"
        onclick={confirm}
        class="flex h-9 w-9 items-center justify-center rounded-full bg-brand-600 text-white hover:bg-brand-700"
      >
        <Check size={17} />
      </button>
    </div>

    <div class="px-4 pb-2">
      <input
        bind:value={typed}
        placeholder="Type a date"
        autocomplete="off"
        class="w-full rounded-lg bg-zinc-100 px-3 py-2 text-sm outline-none placeholder:text-zinc-400 focus:ring-2 focus:ring-brand-500 dark:bg-zinc-800"
      />
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-4">
      {#if suggestion}
        <button
          onclick={() => apply(suggestion)}
          class="flex w-full items-center gap-3 rounded-lg py-2.5 text-left hover:bg-zinc-50 dark:hover:bg-zinc-800/60"
        >
          <CalendarDays size={18} class="text-brand-600" />
          <span class="text-sm font-medium">{dayLabel(suggestion)}</span>
          <span class="ml-auto text-sm text-zinc-400">{weekdayAbbrev(suggestion)} {suggestion}</span>
        </button>
      {/if}

      <!-- quick options -->
      {#each options as option (option.key)}
        {@const Icon = optionIcons[option.key]}
        <button
          onclick={() => apply(option.date)}
          class="flex w-full items-center gap-3 py-3 text-left hover:bg-zinc-50 dark:hover:bg-zinc-800/60"
        >
          <Icon size={18} class={optionColors[option.key]} />
          <span class="text-sm font-medium">{option.label}</span>
          {#if option.date}
            <span class="ml-auto text-sm text-zinc-400">{weekdayAbbrev(option.date)}</span>
          {/if}
        </button>
      {/each}

      <!-- weekday header -->
      <div
        class="sticky top-0 grid grid-cols-7 border-b border-zinc-100 bg-white py-1.5 dark:border-zinc-800 dark:bg-zinc-900"
      >
        {#each ['S', 'M', 'T', 'W', 'T', 'F', 'S'] as wd, i (i)}
          <span class="text-center text-xs font-medium text-zinc-400">{wd}</span>
        {/each}
      </div>

      <!-- month grids -->
      {#each months as { year, month } (year * 100 + month)}
        <p class="py-2 text-center text-sm font-semibold">{monthLabel(year, month, today)}</p>
        {#each monthGrid(year, month, today) as week, wi (wi)}
          <div class="grid grid-cols-7">
            {#each week as day, di (di)}
              {#if day}
                <button
                  onclick={() => (selectedDay = day)}
                  disabled={day < today}
                  class="mx-auto flex h-10 w-10 items-center justify-center rounded-full text-sm {dayClasses(day)}"
                >
                  {Number(day.slice(8))}
                </button>
              {:else}
                <span></span>
              {/if}
            {/each}
          </div>
        {/each}
      {/each}
    </div>

    {#if allowsTime}
      <div class="flex items-center gap-3 border-t border-zinc-100 px-4 py-3 dark:border-zinc-800">
        <Clock size={18} class="text-zinc-400" />
        <span class="text-sm font-medium">Time</span>
        <input
          type="time"
          value={selectedTime}
          onchange={onTimeChange}
          class="ml-auto rounded-lg border border-zinc-200 bg-transparent px-2 py-1 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
        />
        {#if selectedTime}
          <button
            aria-label="Clear time"
            onclick={() => (selectedTime = '')}
            class="p-1 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
          >
            <X size={15} />
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>
