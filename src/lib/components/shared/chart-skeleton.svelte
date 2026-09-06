<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';

  let {
    withFilters = false,
    class: className = ''
  }: {
    withFilters?: boolean;
    class?: string;
  } = $props();
</script>

<Card.Root class={className}>
  <Card.Header>
    <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <div class="flex min-w-0 flex-col gap-1">
        <Skeleton class="h-5 w-40" />
        <Skeleton class="h-4 w-56" />
      </div>
      {#if withFilters}
        <div class="flex flex-wrap gap-1">
          {#each Array(6) as _, i (i)}
            <Skeleton class="h-7 w-10" />
          {/each}
        </div>
      {/if}
    </div>
  </Card.Header>
  <Card.Content>
    <Skeleton class="h-72 w-full" />
  </Card.Content>
</Card.Root>
