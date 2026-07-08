<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import type { Snippet } from 'svelte';

  let {
    icon,
    title,
    description,
    action,
    card = false,
    class: className = ''
  }: {
    icon?: Snippet;
    title?: string;
    description?: string;
    action?: Snippet;
    /** Wrap in a borderless card so the empty state matches table shells. */
    card?: boolean;
    class?: string;
  } = $props();
</script>

{#snippet inner()}
  <div class="flex flex-col items-center gap-3 py-10 text-center {className}">
    {#if icon}
      <div class="flex size-12 items-center justify-center rounded-full bg-muted [&_svg]:size-6">
        {@render icon()}
      </div>
    {/if}
    <div class="flex flex-col gap-1">
      {#if title}
        <p class="font-medium text-foreground">{title}</p>
      {/if}
      {#if description}
        <p class="text-sm text-muted-foreground">{description}</p>
      {/if}
    </div>
    {#if action}
      {@render action()}
    {/if}
  </div>
{/snippet}

{#if card}
  <Card.Root>
    <Card.Content class="p-0">
      {@render inner()}
    </Card.Content>
  </Card.Root>
{:else}
  {@render inner()}
{/if}
