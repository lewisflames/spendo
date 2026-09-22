<script lang="ts">
  import { Skeleton } from '$lib/components/ui/skeleton';
</script>

<div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-3">
  <div class="flex flex-1 flex-col gap-1">
    <Skeleton class="h-9 w-48" />
    <Skeleton class="h-5 w-64" />
  </div>
  <div class="flex items-center gap-2">
    <Skeleton class="h-9 w-28" />
    <Skeleton class="h-9 w-24" />
  </div>
</div>
