<script lang="ts">
  import { ArrowLeft, ArrowRight, Frown, RefreshCw, SearchX } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { Button } from '$lib/components/ui/button';
  import { dbSession } from '$lib/stores/db.svelte';

  const isNotFound = $derived(page.status === 404);

  function goBack() {
    if (window.history.length > 1) window.history.back();
    else void goto(dbSession.value ? resolve('/transactions') : resolve('/'));
  }
</script>

<svelte:head>
  <title>{page.status} — Spendo</title>
</svelte:head>

<div class="flex flex-col gap-6">
  <div class="flex flex-col items-center justify-center gap-4 py-16 text-center">
    {#if isNotFound}
      <SearchX aria-hidden="true" class="size-16 text-muted-foreground" />
      <h1 class="text-6xl font-bold tracking-tight">404</h1>
      <h2 class="text-xl font-semibold">Page not found</h2>
      <p class="max-w-md text-sm text-muted-foreground">
        The page you're looking for doesn't exist or has been moved.
      </p>
    {:else}
      <Frown aria-hidden="true" class="size-16 text-destructive" />
      <h1 class="text-6xl font-bold tracking-tight">{page.status}</h1>
      <h2 class="text-xl font-semibold">Something went wrong</h2>
      <p class="max-w-md text-sm text-muted-foreground">
        {page.error?.message ?? 'An unexpected error occurred.'}
      </p>
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
      {#if dbSession.value}
        <Button onclick={() => goto(resolve('/transactions'))} variant="outline">
          <ArrowRight aria-hidden="true" class="size-4" />
          Transactions
        </Button>
      {:else}
        <Button onclick={() => goto(resolve('/'))} variant="outline">
          <ArrowRight aria-hidden="true" class="size-4" />
          Welcome
        </Button>
      {/if}
    </div>
  </div>
</div>
