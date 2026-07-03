<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { ArrowLeft, ArrowRight } from '@lucide/svelte';

  let {
    page = $bindable(1),
    totalPages = 1
  }: {
    page?: number;
    totalPages?: number;
  } = $props();
</script>

<div class="flex items-center justify-between">
  <p class="text-sm text-muted-foreground">
    Page {page} of {totalPages}
  </p>
  <div class="flex items-center gap-2">
    <Button variant="outline" size="sm" disabled={page <= 1} onclick={() => page--}>
      <ArrowLeft aria-hidden="true" class="mr-1 size-3.5" />
      Previous
    </Button>
    <Button variant="outline" size="sm" disabled={page >= totalPages} onclick={() => page++}>
      Next
      <ArrowRight aria-hidden="true" class="ml-1 size-3.5" />
    </Button>
  </div>
</div>
