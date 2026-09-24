<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import type { Snippet } from 'svelte';

  let {
    icon,
    title = '',
    value,
    valueClass = '',
    loading = false
  }: {
    icon?: Snippet;
    title?: string;
    value?: string | Snippet;
    valueClass?: string;
    loading?: boolean;
  } = $props();
</script>

<Card.Root>
  {#if loading}
    <Card.Header>
      <Skeleton class="h-4 w-20" />
    </Card.Header>
    <Card.Content>
      <Skeleton class="h-7 w-28" />
    </Card.Content>
  {:else}
    {#if icon}
      <IconCardHeader {title}>
        {#snippet icon()}
          {@render icon()}
        {/snippet}
      </IconCardHeader>
    {:else}
      <Card.Header>
        <Card.Title>{title}</Card.Title>
      </Card.Header>
    {/if}
    {#if value !== undefined}
      <Card.Content>
        {#if typeof value === 'function'}
          <div class="text-2xl font-bold tracking-tight tabular-nums {valueClass}">
            {@render value()}
          </div>
        {:else}
          <p class="text-2xl font-bold tracking-tight tabular-nums {valueClass}">{value}</p>
        {/if}
      </Card.Content>
    {/if}
  {/if}
</Card.Root>
