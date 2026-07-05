<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { TrendingUp, TrendingDown, Wallet, ArrowLeftRight } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchDashboardData } from '$lib/dashboard.remote';
  import { goto } from '$app/navigation';
  import type { DashboardData } from '$lib/server/dashboard-data';
  import NetWorthChart from '$lib/components/dashboard/net-worth-chart.svelte';
  import CategoryBar from '$lib/components/dashboard/category-bar.svelte';
  import RecentTransactions from '$lib/components/dashboard/recent-transactions.svelte';
  import { formatCompact } from '$lib/format.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';

  const rangeOptions = [
    { label: '1M', value: '1M', monthCount: 1 },
    { label: '3M', value: '3M', monthCount: 3 },
    { label: '6M', value: '6M', monthCount: 6 },
    { label: '1Y', value: '1Y', monthCount: 12 },
    { label: 'All', value: 'All', monthCount: null }
  ] as const;

  let selectedRange = $state(appSettings.value.dashboardRange);

  const periodSuffixMap: Record<string, string> = {
    '1M': '(mo)',
    '3M': '(3mo)',
    '6M': '(6mo)',
    '1Y': '(12mo)',
    All: '(all)'
  };

  const chartPeriodMap: Record<string, string> = {
    '1M': 'This month',
    '3M': 'Last 3 months',
    '6M': 'Last 6 months',
    '1Y': 'Last 12 months',
    All: 'All time'
  };

  let periodSuffix = $derived(periodSuffixMap[selectedRange]);
  let chartPeriod = $derived(chartPeriodMap[selectedRange]);
  let currentOption = $derived(rangeOptions.find((o) => o.value === selectedRange)!);

  let data = $state<DashboardData | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!dbSession.value) return;

    error = null;
    let cancelled = false;

    fetchDashboardData({ monthCount: currentOption.monthCount })
      .then((result) => {
        if (cancelled) return;
        data = result;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error = e instanceof Error ? e.message : 'Failed to load dashboard data';
      });

    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (!dbSession.value) goto('/');
  });
</script>

<svelte:head>
  <title>Dashboard - Spendo</title>
</svelte:head>

<div class="flex flex-col gap-6">
  <div class="flex items-center justify-between gap-4">
    <div class="flex flex-col gap-1">
      <h2 class="text-3xl font-bold tracking-tight">Dashboard</h2>
      <p class="text-sm text-muted-foreground">Financial overview of your data</p>
    </div>

    <div class="flex gap-1" role="group" aria-label="Time range">
      {#each rangeOptions as opt (opt.value)}
        <Button
          variant={selectedRange === opt.value ? 'default' : 'outline'}
          size="sm"
          onclick={() => (selectedRange = opt.value)}
        >
          {opt.label}
        </Button>
      {/each}
    </div>
  </div>

  {#if data}
    <!-- Error banner above charts when refetch fails but old data exists -->
    {#if error}
      <div
        class="rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive"
      >
        {error}
      </div>
    {/if}
    <!-- KPI cards -->
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <Wallet
              aria-hidden="true"
              class="size-4 {data.netWorth >= 0
                ? 'text-emerald-600 dark:text-emerald-400'
                : 'text-red-600 dark:text-red-400'}"
            />
            <Card.Title>Net Worth</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p
            class="text-3xl font-bold {data.netWorth >= 0
              ? 'text-emerald-600 dark:text-emerald-400'
              : 'text-red-600 dark:text-red-400'}"
          >
            {data.netWorth >= 0 ? '+' : ''}{formatCompact(data.netWorth)}
          </p>
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <TrendingUp aria-hidden="true" class="size-4 text-emerald-600 dark:text-emerald-400" />
            <Card.Title>Income {periodSuffix}</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p class="text-3xl font-bold text-emerald-600 dark:text-emerald-400">
            +{formatCompact(data.currentMonthIncome)}
          </p>
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <TrendingDown aria-hidden="true" class="size-4 text-red-600 dark:text-red-400" />
            <Card.Title>Expenses {periodSuffix}</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p class="text-3xl font-bold text-red-600 dark:text-red-400">
            &minus;{formatCompact(data.currentMonthExpenses)}
          </p>
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <ArrowLeftRight
              aria-hidden="true"
              class="size-4 {data.currentMonthNet >= 0
                ? 'text-emerald-600 dark:text-emerald-400'
                : 'text-red-600 dark:text-red-400'}"
            />
            <Card.Title>Cash Flow {periodSuffix}</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p
            class="text-3xl font-bold {data.currentMonthNet >= 0
              ? 'text-emerald-600 dark:text-emerald-400'
              : 'text-red-600 dark:text-red-400'}"
          >
            {data.currentMonthNet >= 0 ? '+' : ''}{formatCompact(data.currentMonthNet)}
          </p>
        </Card.Content>
      </Card.Root>
    </div>

    <!-- Charts + Recent transactions grid -->
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
      <Card.Root>
        <Card.Header>
          <Card.Title>Net Worth Trend</Card.Title>
          <Card.Description>{chartPeriod}</Card.Description>
        </Card.Header>
        <Card.Content>
          <NetWorthChart data={data.netWorthTrend} />
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header>
          <Card.Title>Top Spending Categories</Card.Title>
          <Card.Description>{chartPeriod}</Card.Description>
        </Card.Header>
        <Card.Content>
          {#if data.topCategories.length === 0}
            <p class="py-4 text-center text-sm text-muted-foreground">No expense data found</p>
          {:else}
            <CategoryBar data={data.topCategories} />
          {/if}
        </Card.Content>
      </Card.Root>

      <!-- Recent transactions -->
      <div class="col-span-2">
        <RecentTransactions data={data.recentTransactions} />
      </div>
    </div>
  {:else if error}
    <Card.Root class="border-destructive">
      <Card.Content class="py-6 text-center text-sm text-destructive">
        {error}
      </Card.Content>
    </Card.Root>
  {:else}
    <!-- KPI skeletons -->
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
      {#each Array(4) as _, i (i)}
        <Card.Root>
          <Card.Header>
            <div class="flex items-center gap-2">
              <Skeleton class="size-4 rounded" />
              <Skeleton class="h-4 w-24" />
            </div>
          </Card.Header>
          <Card.Content>
            <Skeleton class="h-8 w-28" />
          </Card.Content>
        </Card.Root>
      {/each}
    </div>

    <!-- Skeleton grid -->
    <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
      {#each Array(4) as _, i (i)}
        <Card.Root class={i >= 2 ? 'lg:col-span-2' : ''}>
          <Card.Header>
            <Skeleton class="h-5 w-36" />
          </Card.Header>
          <Card.Content>
            <Skeleton class="h-48 w-full rounded-lg" />
          </Card.Content>
        </Card.Root>
      {/each}
    </div>
  {/if}
</div>
