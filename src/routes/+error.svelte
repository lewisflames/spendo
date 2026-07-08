<script lang="ts">
  import { ArrowLeft, Frown, RefreshCw, SearchX } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { Button } from '$lib/components/ui/button';

  const isNotFound = $derived(page.status === 404);
</script>

<svelte:head>
  <title>{page.status} — Spendo</title>
</svelte:head>

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
      <RefreshCw aria-hidden="true" class="mr-1.5 size-3.5" />
      Try again
    </Button>
  {/if}
  <Button onclick={() => goto(resolve('/'))} variant="outline">
    <ArrowLeft aria-hidden="true" class="mr-1.5 size-3.5" />
    Go home
  </Button>
</div>
