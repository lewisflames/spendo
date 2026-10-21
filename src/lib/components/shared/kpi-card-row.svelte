<script lang="ts">
  import KpiCard from './kpi-card.svelte';

  let {
    count = 3,
    gridClass = ''
  }: {
    count?: number;
    /** Overrides the responsive grid classes (must match the loaded KPI grid). */
    gridClass?: string;
  } = $props();

  const GRID_COLS: Record<number, string> = {
    2: '@xl:grid-cols-2',
    3: '@xl:grid-cols-2 @3xl:grid-cols-3',
    4: '@xl:grid-cols-2 @4xl:grid-cols-4',
    6: '@xl:grid-cols-2 @3xl:grid-cols-3'
  };
</script>

<div class="grid grid-cols-1 gap-6 {(gridClass || GRID_COLS[count]) ?? GRID_COLS[3]}">
  {#each Array(count) as _, i (i)}
    <KpiCard loading />
  {/each}
</div>
