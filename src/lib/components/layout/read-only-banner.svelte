<script lang="ts">
  import { resolve } from '$app/paths';
  import { Lock } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';

  // Shown while the open book was saved by a newer Spendo (schema version
  // above this build's): the backend opened it read-only and refuses writes.
</script>

<div
  role="status"
  class="mb-(--page-gap) flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-warning/40 bg-warning/10 px-3.5 py-2.5 text-sm"
>
  <Lock aria-hidden="true" class="size-4 shrink-0 text-warning" />
  <p class="min-w-0 flex-1">
    <span class="font-medium">Read-only.</span>
    This book was saved by a newer Spendo. Update to edit it
  </p>
  <Button variant="outline" size="sm" href={resolve('/about')}>Check for updates</Button>
</div>
