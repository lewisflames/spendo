<script lang="ts">
  import { Skeleton } from '$lib/components/ui/skeleton';

  let {
    withActions = true,
    withDescription = true
  }: {
    withActions?: boolean;
    withDescription?: boolean;
  } = $props();
</script>

<div class="flex items-center justify-between gap-4">
  <div class="flex flex-1 flex-col gap-1">
    <Skeleton class="h-9 w-48" />
    {#if withDescription}
      <Skeleton class="h-5 w-64" />
    {/if}
  </div>
  {#if withActions}
    <div class="flex items-center gap-2">
      <Skeleton class="h-9 w-28" />
      <Skeleton class="h-9 w-24" />
    </div>
  {/if}
</div>
