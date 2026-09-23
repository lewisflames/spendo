<script lang="ts">
  import { EMPTY_DASH } from '$lib/utils/format.svelte';

  /**
   * Standard missing-value placeholder for tables: muted extra-small dash
   * so nulls read the same in every numeric column.
   */
  let { value = EMPTY_DASH }: { value?: string } = $props();
</script>

<span class="text-xs text-muted-foreground">{value}</span>
