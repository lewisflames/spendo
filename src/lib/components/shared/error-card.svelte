<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { CircleAlert, RefreshCw } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  let {
    message,
    onRetry,
    children
  }: {
    message?: string;
    onRetry?: () => void;
    children?: Snippet;
  } = $props();
</script>

<Card.Root class="border-destructive/50" role="alert">
  <Card.Content class="flex items-center gap-2 py-3 text-sm text-destructive">
    <CircleAlert aria-hidden="true" class="size-4 shrink-0" />
    <span class="min-w-0 flex-1">
      {#if message}
        {message}
      {/if}
      {#if children}
        {@render children()}
      {/if}
    </span>
    {#if onRetry}
      <Button variant="outline" size="sm" onclick={onRetry}>
        <RefreshCw aria-hidden="true" class="size-3" />
        Retry
      </Button>
    {/if}
  </Card.Content>
</Card.Root>
