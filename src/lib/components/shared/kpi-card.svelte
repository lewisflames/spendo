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
    loading = false,
    href,
    linkLabel
  }: {
    icon?: Snippet;
    title?: string;
    value?: string | Snippet;
    valueClass?: string;
    loading?: boolean;
    /** Makes the whole card a link to the page behind the number. */
    href?: string;
    /** Accessible name for the card link; defaults to `title`. */
    linkLabel?: string;
  } = $props();
</script>

<Card.Root class={href ? 'relative transition-colors hover:bg-muted/50' : undefined}>
  {#if href && !loading}
    <!-- Stretched link: the whole card is clickable, middle-click and keyboard included. -->
    <!-- eslint-disable svelte/no-navigation-without-resolve -- callers pass resolved hrefs -->
    <a
      {href}
      aria-label={linkLabel ?? title}
      class="absolute inset-0 z-10 rounded-[inherit] focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none focus-visible:ring-inset"
    ></a>
    <!-- eslint-enable svelte/no-navigation-without-resolve -->
  {/if}
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
