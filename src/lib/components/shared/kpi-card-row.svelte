<script lang="ts">
  import KpiCard from './kpi-card.svelte';

  let { count = 3 }: { count?: number } = $props();
</script>

<div class="grid grid-cols-1 gap-6 sm:grid-cols-3">
  {#each Array(count) as _, i (i)}
    <KpiCard loading />
  {/each}
</div>
