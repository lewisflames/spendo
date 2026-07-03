<script lang="ts">
  import { ArrowLeft, Frown, SearchX } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { Button } from '$lib/components/ui/button';
</script>

<svelte:head>
  <title>{page.status} — Spendo</title>
</svelte:head>

{#if page.status === 404}
  <div class="flex flex-col items-center justify-center gap-4 py-16 text-center">
    <SearchX aria-hidden="true" class="size-16 text-muted-foreground" />
    <h1 class="text-6xl font-bold tracking-tight">404</h1>
    <h2 class="text-xl font-semibold">Page not found</h2>
    <p class="max-w-md text-sm text-muted-foreground">
      The page you're looking for doesn't exist or has been moved.
    </p>
    <Button onclick={() => goto(resolve('/'))} variant="outline">
      <ArrowLeft aria-hidden="true" class="mr-1.5 size-3.5" />
      Go home
    </Button>
  </div>
{:else}
  <div class="flex flex-col items-center justify-center gap-4 py-16 text-center">
    <Frown aria-hidden="true" class="size-16 text-destructive" />
    <h1 class="text-6xl font-bold tracking-tight">{page.status}</h1>
    <h2 class="text-xl font-semibold">Something went wrong</h2>
    <p class="max-w-md text-sm text-muted-foreground">
      {page.error?.message ?? 'An unexpected error occurred.'}
    </p>
    <Button onclick={() => goto(resolve('/'))} variant="outline">
      <ArrowLeft aria-hidden="true" class="mr-1.5 size-3.5" />
      Go home
    </Button>
  </div>
{/if}
