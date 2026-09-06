<script lang="ts">
  import { Skeleton } from '$lib/components/ui/skeleton';

  let {
    class: className = ''
  }: {
    class?: string;
  } = $props();
</script>

<div class="flex flex-wrap items-center justify-between gap-2 {className}">
  <Skeleton class="h-9 w-full max-w-xs" />
  <div class="flex items-center gap-2">
    <Skeleton class="h-9 w-28" />
  </div>
</div>
