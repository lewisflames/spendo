<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import type { Snippet } from 'svelte';

  let {
    icon,
    title,
    description,
    action
  }: {
    icon: Snippet;
    title: string;
    description?: string;
    action?: Snippet;
  } = $props();
</script>

<Card.Header>
  <div class="flex items-center gap-2">
    {@render icon()}
    <Card.Title>{title}</Card.Title>
  </div>
  {#if action}
    <div
      data-slot="card-action"
      class="col-start-2 row-span-2 row-start-1 self-start justify-self-end"
    >
      {@render action()}
    </div>
  {/if}
  {#if description}
    <Card.Description>{description}</Card.Description>
  {/if}
</Card.Header>
