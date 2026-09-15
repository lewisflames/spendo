<script lang="ts">
  import { Skeleton } from '$lib/components/ui/skeleton';
  import ChartSkeleton from '$lib/components/shared/chart-skeleton.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import PageHeaderSkeleton from '$lib/components/shared/page-header-skeleton.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';

  /**
   * Full-page loading placeholder. Each variant mirrors the loaded layout of
   * its page(s) so first paint doesn't jump when data arrives. Keep counts in
   * sync with the loaded grids/tables they stand in for.
   */
  let {
    variant
  }: {
    variant:
      | 'welcome'
      | 'accounts'
      | 'transactions'
      | 'transaction-detail'
      | 'form'
      | 'budget-detail'
      | 'portfolio'
      | 'holding'
      | 'scheduled';
  } = $props();
</script>

{#if variant === 'welcome'}
  <div class="m-auto flex w-full max-w-2xl flex-col gap-10 py-8">
    <div class="flex flex-col items-center gap-5">
      <Skeleton class="size-24 rounded-2xl" />
      <div class="flex w-full flex-col items-center gap-3">
        <Skeleton class="h-4 w-44" />
        <Skeleton class="h-10 w-80 max-w-full" />
        <Skeleton class="h-4 w-full max-w-md" />
      </div>
    </div>
    <div class="flex flex-col items-center gap-3">
      <div class="flex w-full flex-col gap-2 sm:w-auto sm:flex-row">
        <Skeleton class="h-9 w-full rounded-lg sm:w-40" />
        <Skeleton class="h-9 w-full rounded-lg sm:w-40" />
      </div>
      <Skeleton class="h-3 w-36" />
    </div>
  </div>
{:else if variant === 'accounts'}
  <KpiCardRow count={4} />
  <TableSkeleton cols={3} />
{:else if variant === 'transactions'}
  <div class="flex flex-col gap-4">
    <TableSkeleton cols={3} />
    <div class="flex items-center justify-between">
      <Skeleton class="h-5 w-32" />
      <div class="flex items-center gap-2">
        <Skeleton class="h-8 w-24" />
        <Skeleton class="h-8 w-16" />
      </div>
    </div>
  </div>
{:else if variant === 'transaction-detail'}
  <PageHeaderSkeleton />
  <KpiCardRow gridClass="sm:grid-cols-2 lg:grid-cols-3" />
  <TableSkeleton cols={4} rows={3} />
{:else if variant === 'budget-detail'}
  <KpiCardRow count={4} />
  <!-- Mirrors the pacing card shape. -->
  <div class="rounded-lg border p-4">
    <div class="flex flex-wrap items-baseline justify-between gap-2">
      <Skeleton class="h-4 w-52" />
      <Skeleton class="h-4 w-44" />
    </div>
    <div class="mt-3 flex items-center gap-3">
      <Skeleton class="h-2 flex-1" />
      <Skeleton class="h-4 w-12" />
    </div>
    <Skeleton class="mt-2 h-3 w-72" />
  </div>
  <TableSkeleton cols={6} rows={6} />
{:else if variant === 'portfolio'}
  <!-- The holdings table renders its own skeleton via HoldingsTable loading. -->
  <KpiCardRow />
  <!-- Matches the loaded performance + allocation layout. -->
  <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
    <ChartSkeleton class="lg:col-span-2" withFilters />
    <ChartSkeleton />
  </div>
{:else if variant === 'holding'}
  <PageHeaderSkeleton />
  <KpiCardRow count={4} />
  <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
    <ChartSkeleton withFilters />
    <ChartSkeleton withFilters />
  </div>
  <TableSkeleton cols={6} rows={3} />
  <TableSkeleton cols={5} rows={4} />
{:else if variant === 'scheduled'}
  <PageHeaderSkeleton />
  <TableSkeleton cols={7} rows={4} />
{:else if variant === 'form'}
  <PageHeaderSkeleton />
  <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
    <Skeleton class="order-2 h-72 w-full rounded-lg lg:order-1" />
    <Skeleton class="order-1 h-80 w-full rounded-lg lg:order-2" />
  </div>
{/if}
