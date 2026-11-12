<script>
  import { data, reportContent, blockUser, toast, mutationSession, sessionValid } from '../state.svelte.js';
  import { X } from '@lucide/svelte';

  // kind: 'comment' | 'task' | 'attachment' | 'user', or null for block-only.
  let { kind = null, targetId = null, userId, userName, onclose, ondone = () => {} } = $props();

  const reasons = [
    ['spam', 'Spam'],
    ['harassment', 'Harassment or bullying'],
    ['hate', 'Hate speech'],
    ['sexual', 'Sexual content'],
    ['violence', 'Violence or threats'],
    ['illegal', 'Illegal activity'],
    ['other', 'Something else']
  ];
  const noun = { comment: 'comment', task: 'task', attachment: 'attachment', user: 'person' };

  let reason = $state('');
  let details = $state('');
  let alsoBlock = $state(false);
  let busy = $state(false);
  let reported = $state(false); // the report went through; a retry only blocks
  const canBlock = $derived(userId !== data.user?.id);

  async function submit(e) {
    e.preventDefault();
    busy = true;
    const session = mutationSession();
    try {
      if (kind && !reported) {
        await reportContent(kind, targetId, reason, details);
        reported = true;
      }
      // Signed out (or into another account) while the report was in flight.
      if (!sessionValid(session)) return;
      if (!kind || (alsoBlock && canBlock)) {
        try {
          await blockUser(userId);
        } catch (err) {
          if (!sessionValid(session)) return;
          if (!kind) throw err;
          toast(`Report sent, but blocking ${userName} failed: ${err.message}`);
          ondone({ blocked: false });
          onclose();
          return;
        }
      }
      if (!sessionValid(session)) return;
      toast(
        kind
          ? alsoBlock && canBlock
            ? `Reported and blocked ${userName}`
            : 'Thanks — we review reports within 24 hours'
          : `Blocked ${userName}`
      );
      ondone({ blocked: !kind || (alsoBlock && canBlock) });
      onclose();
    } catch (err) {
      if (sessionValid(session)) toast(err.message);
    } finally {
      busy = false;
    }
  }

  // Closing mid-submit would leave a block running the user thinks they cancelled.
  function dismiss() {
    if (!busy) onclose();
  }

  function onkeydown(e) {
    if (e.key === 'Escape') dismiss();
  }
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-[60] flex items-end justify-center bg-black/40 sm:items-center sm:p-6"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && dismiss()}
>
  <form
    onsubmit={submit}
    class="max-h-[90dvh] w-full overflow-y-auto overscroll-contain rounded-t-2xl bg-white p-5 shadow-xl sm:max-w-md sm:rounded-2xl dark:bg-zinc-900"
    style="padding-bottom: max(1.25rem, env(safe-area-inset-bottom))"
  >
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold">
        {kind ? `Report ${noun[kind]}` : `Block ${userName}?`}
      </h2>
      <button type="button" aria-label="Close" onclick={dismiss} disabled={busy} class="p-1 text-zinc-400 hover:text-zinc-600">
        <X size={18} />
      </button>
    </div>

    {#if kind}
      <p class="mt-1 text-xs text-zinc-500">
        Tell us what's wrong. Reports are confidential; we review them within 24 hours and remove content or accounts
        that break the Terms.
      </p>
      <fieldset class="mt-3 space-y-1">
        <legend class="sr-only">Reason</legend>
        {#each reasons as [value, label] (value)}
          <label class="flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-sm hover:bg-zinc-50 dark:hover:bg-zinc-800">
            <input type="radio" name="reason" {value} bind:group={reason} required class="accent-brand-600" />
            {label}
          </label>
        {/each}
      </fieldset>
      <textarea
        bind:value={details}
        rows="2"
        maxlength="2000"
        placeholder="Anything else we should know? (optional)"
        class="mt-2 w-full rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
      ></textarea>
      {#if canBlock}
      <label class="mt-2 flex items-start gap-2.5 text-sm">
        <input type="checkbox" bind:checked={alsoBlock} class="mt-0.5 accent-brand-600" />
        <span>
          Also block {userName}
          <span class="block text-xs text-zinc-400">They'll be removed from your projects, you'll leave theirs, and you won't see their comments.</span>
        </span>
      </label>
      {/if}
    {:else}
      <p class="mt-2 text-sm text-zinc-500">
        {userName} will be removed from projects you own and you'll leave projects they own. Neither of you can share
        projects with the other, and you won't see their comments. You can unblock them in Settings.
      </p>
    {/if}

    <div class="mt-4 flex gap-2">
      <button type="button" onclick={dismiss} disabled={busy} class="flex-1 rounded-lg border border-zinc-200 px-3 py-2 text-sm dark:border-zinc-700">
        Cancel
      </button>
      <button
        type="submit"
        disabled={busy || (kind && !reason)}
        class="flex-1 rounded-lg bg-red-600 px-3 py-2 text-sm font-semibold text-white hover:bg-red-700 disabled:opacity-50"
      >
        {busy ? 'One moment…' : kind ? 'Send report' : 'Block'}
      </button>
    </div>
  </form>
</div>
