<script lang="ts">
  import KpiCard from './kpi-card.svelte';

  let { count = 3 }: { count?: number } = $props();

  const GRID_COLS: Record<number, string> = {
    2: 'sm:grid-cols-2',
    3: 'sm:grid-cols-3',
    4: 'sm:grid-cols-2 lg:grid-cols-4'
  };
</script>

<div class="grid grid-cols-1 gap-6 {GRID_COLS[count] ?? 'sm:grid-cols-' + count}">
  {#each Array(count) as _, i (i)}
    <KpiCard loading />
  {/each}
</div>
