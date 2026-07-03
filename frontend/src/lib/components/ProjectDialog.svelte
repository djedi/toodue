<script>
  import {
    data,
    updateProject,
    deleteProject,
    shareProject,
    removeMember,
    toast
  } from '../state.svelte.js';
  import { X, UserPlus, Trash2 } from '@lucide/svelte';

  let { project, onclose } = $props();

  let name = $state(project.name);
  let email = $state('');
  let busy = $state(false);

  const isOwner = $derived(project.owner_id === data.user.id);

  // Valid parents: not the inbox, not itself, not anything inside its own subtree.
  const possibleParents = $derived.by(() => {
    const descendants = new Set([project.id]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const p of data.projects) {
        if (!descendants.has(p.id) && p.parent_id && descendants.has(p.parent_id)) {
          descendants.add(p.id);
          grew = true;
        }
      }
    }
    return data.projects.filter((p) => !p.is_inbox && !descendants.has(p.id));
  });

  async function setParent(value) {
    try {
      await updateProject(project.id, { parent_id: value ? Number(value) : null });
    } catch (err) {
      toast(err.message);
    }
  }

  async function rename() {
    if (!name.trim() || name === project.name) return;
    try {
      await updateProject(project.id, { name });
    } catch (err) {
      toast(err.message);
    }
  }

  async function invite(e) {
    e.preventDefault();
    if (!email.trim() || busy) return;
    busy = true;
    try {
      await shareProject(project.id, email);
      toast(`Shared with ${email}`);
      email = '';
    } catch (err) {
      toast(err.message);
    } finally {
      busy = false;
    }
  }

  async function remove(memberId) {
    try {
      await removeMember(project.id, memberId);
      if (memberId === data.user.id) onclose();
    } catch (err) {
      toast(err.message);
    }
  }

  async function destroy() {
    if (!confirm(`Delete "${project.name}" and all of its tasks?`)) return;
    try {
      await deleteProject(project.id);
      onclose();
    } catch (err) {
      toast(err.message);
    }
  }

  function onkeydown(e) {
    if (e.key === 'Escape') onclose();
  }
</script>

<svelte:window {onkeydown} />

<div
  class="fixed inset-0 z-40 flex items-end justify-center bg-black/40 sm:items-center sm:p-6"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onclose()}
>
  <div class="w-full rounded-t-2xl bg-white p-5 shadow-xl sm:max-w-md sm:rounded-2xl dark:bg-zinc-900">
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold">Project settings</h2>
      <button aria-label="Close" onclick={onclose} class="p-1 text-zinc-400 hover:text-zinc-600">
        <X size={18} />
      </button>
    </div>

    <label class="mt-4 block">
      <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Name</span>
      <input
        bind:value={name}
        onblur={rename}
        onkeydown={(e) => e.key === 'Enter' && e.target.blur()}
        class="mt-1 w-full rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
      />
    </label>

    <label class="mt-4 block">
      <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Parent project</span>
      <select
        value={project.parent_id ?? ''}
        onchange={(e) => setParent(e.target.value)}
        class="mt-1 w-full rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
      >
        <option value="">None (top level)</option>
        {#each possibleParents as p (p.id)}
          <option value={p.id}>{p.name}</option>
        {/each}
      </select>
    </label>

    <div class="mt-5">
      <span class="text-xs font-semibold tracking-wide text-zinc-400 uppercase">Members</span>
      <ul class="mt-1 space-y-1">
        {#each project.members ?? [] as member (member.id)}
          <li class="flex items-center gap-2.5 py-1.5">
            <div class="flex h-7 w-7 flex-none items-center justify-center rounded-full bg-zinc-100 text-xs font-bold text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
              {member.name.slice(0, 1).toUpperCase()}
            </div>
            <div class="min-w-0 flex-1">
              <span class="block truncate text-sm font-medium">
                {member.name}{member.id === data.user.id ? ' (you)' : ''}
              </span>
              <span class="block truncate text-xs text-zinc-400">{member.email}</span>
            </div>
            {#if member.role === 'owner'}
              <span class="text-[11px] font-medium text-zinc-400 uppercase">Owner</span>
            {:else if isOwner || member.id === data.user.id}
              <button
                onclick={() => remove(member.id)}
                class="text-xs text-zinc-400 hover:text-red-600"
              >
                {member.id === data.user.id ? 'Leave' : 'Remove'}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
      <form onsubmit={invite} class="mt-2 flex gap-2">
        <input
          bind:value={email}
          type="email"
          placeholder="Invite by email"
          class="min-w-0 flex-1 rounded-lg border border-zinc-200 bg-transparent px-3 py-2 text-sm outline-none focus:border-brand-500 dark:border-zinc-700"
        />
        <button
          type="submit"
          disabled={!email.trim() || busy}
          class="flex items-center gap-1.5 rounded-lg bg-brand-600 px-3 py-2 text-sm font-semibold text-white disabled:opacity-40"
        >
          <UserPlus size={15} /> Share
        </button>
      </form>
      <p class="mt-1.5 text-xs text-zinc-400">
        They'll see this project and its tasks instantly, and changes sync live.
      </p>
    </div>

    {#if isOwner}
      <div class="mt-6 border-t border-zinc-100 pt-4 dark:border-zinc-800">
        <button
          onclick={destroy}
          class="flex items-center gap-2 text-sm font-medium text-red-600 hover:underline dark:text-red-400"
        >
          <Trash2 size={15} /> Delete project
        </button>
      </div>
    {/if}
  </div>
</div>
