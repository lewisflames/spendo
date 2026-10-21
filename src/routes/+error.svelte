<script lang="ts">
  import { ArrowLeft, ArrowRight, Frown, RefreshCw, SearchX } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { Button } from '$lib/components/ui/button';
  import { dbSession } from '$lib/stores/db.svelte';
  import { goBack as goBackInApp, navHistory } from '$lib/stores/nav-history.svelte';

  const isNotFound = $derived(page.status === 404);
  const rawError = $derived(page.error?.message ?? '');

  // Prefer in-app history so the button matches the title bar; deep links
  // (empty stack) fall back to the app’s home instead of leaving it.
  function goBack() {
    if (navHistory.value.index > 0) {
      goBackInApp();
    } else {
      void goto(resolve('/'));
    }
  }
</script>

<svelte:head>
  <title>{page.status} · Spendo</title>
</svelte:head>

<div class="page-stack">
  <div class="flex flex-col items-center justify-center gap-4 py-16 text-center">
    {#if isNotFound}
      <SearchX aria-hidden="true" class="size-16 text-muted-foreground" />
      <h1 class="text-5xl font-semibold tracking-tight">404</h1>
      <h2 class="text-xl font-semibold">We can’t find that page</h2>
      <p class="max-w-md text-sm text-muted-foreground">
        That link doesn’t go anywhere anymore. It may have moved.
      </p>
    {:else}
      <Frown aria-hidden="true" class="size-16 text-destructive" />
      <h1 class="text-5xl font-semibold tracking-tight">{page.status}</h1>
      <h2 class="text-xl font-semibold">That didn’t work</h2>
      <p class="max-w-md text-sm text-muted-foreground">
        Something went wrong. Try again or go back. The details below may help if it keeps
        happening.
      </p>
      {#if rawError}
        <details class="max-w-md text-left text-xs text-muted-foreground">
          <summary class="cursor-pointer">Error details</summary>
          <p class="mt-1 break-words">{rawError}</p>
        </details>
      {/if}
      <Button variant="outline" onclick={() => location.reload()}>
        <RefreshCw aria-hidden="true" class="size-4" />
        Try again
      </Button>
    {/if}
    <div class="flex items-center gap-2">
      <Button onclick={goBack} variant="outline">
        <ArrowLeft aria-hidden="true" class="size-4" />
        Go back
      </Button>
      <Button onclick={() => goto(resolve('/'))} variant="outline">
        <ArrowRight aria-hidden="true" class="size-4" />
        {dbSession.value ? 'Overview' : 'Welcome'}
      </Button>
    </div>
  </div>
</div>
