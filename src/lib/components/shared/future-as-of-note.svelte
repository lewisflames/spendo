<script lang="ts">
  import { todayStr } from '$lib/utils/date-utils.svelte';

  let { asOf }: { asOf: string } = $props();

  // Future-dated transactions are excluded backend-side, so a future as-of
  // date silently shows current balances — disclose instead of clamping.
  let isFutureAsOf = $derived(!!asOf && asOf > todayStr());
</script>

{#if isFutureAsOf}
  <p class="text-xs text-muted-foreground">
    Future date selected — future transactions are excluded, so this shows current balances.
  </p>
{/if}
