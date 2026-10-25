<script lang="ts">
  import { onMount } from 'svelte';
  import { FolderOpen, FolderPlus, LoaderCircle } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import Kbd from '$lib/components/shared/kbd.svelte';
  import { isMacPlatform } from '$lib/utils/platform';

  let {
    busy,
    onNew,
    onOpen
  }: {
    busy: 'new' | 'open' | null;
    onNew: () => void;
    onOpen: () => void;
  } = $props();

  // Resolved after mount so prerendered markup matches the hydrated label.
  let modLabel = $state('Ctrl+');
  onMount(() => {
    modLabel = isMacPlatform() ? '⌘' : 'Ctrl+';
  });

  let newButton = $state<HTMLElement | null>(null);
  let didFocus = false;

  // Focus New once on mount; never steal focus later.
  $effect(() => {
    if (!didFocus && newButton) {
      didFocus = true;
      newButton.focus();
    }
  });
</script>

<div class="flex flex-col gap-2 @md:flex-row">
  <Button
    size="lg"
    disabled={busy !== null}
    aria-busy={busy === 'new'}
    aria-keyshortcuts="Meta+N Control+N"
    bind:ref={newButton}
    onclick={onNew}
  >
    {#if busy === 'new'}
      <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
    {:else}
      <FolderPlus aria-hidden="true" class="size-4" />
    {/if}
    {busy === 'new' ? 'Creating…' : 'New book…'}
    <Kbd
      class="border-primary-foreground/25 bg-primary-foreground/10 font-normal text-primary-foreground/80"
    >
      {modLabel}N
    </Kbd>
  </Button>

  <Button
    size="lg"
    variant="outline"
    disabled={busy !== null}
    aria-busy={busy === 'open'}
    aria-keyshortcuts="Meta+O Control+O"
    onclick={onOpen}
  >
    {#if busy === 'open'}
      <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
    {:else}
      <FolderOpen aria-hidden="true" class="size-4" />
    {/if}
    {busy === 'open' ? 'Opening…' : 'Open book…'}
    <Kbd class="font-normal">{modLabel}O</Kbd>
  </Button>
</div>
