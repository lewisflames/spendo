<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchRealizedGains } from '$lib/tauri';
  import { Download, HandCoins, TrendingDown, TrendingUp, Coins } from '@lucide/svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import OverflowMenu from '$lib/components/shared/overflow-menu.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import BudgetYearStepper from '$lib/features/budgets/budget-year-stepper.svelte';
  import { buildYearOptions } from '$lib/features/budgets/budget-utils';
  import RealizedGainsTable from '$lib/features/portfolio/realized-gains-table.svelte';
  import { exportRealizedGainsCsvViaDialog } from '$lib/features/book/import-export-actions';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useUrlParams, yearParam } from '$lib/hooks/use-url-params.svelte';
  import { formatSessionCurrency as fmt, signColor } from '$lib/utils/format.svelte';

  const meta = PAGE_META.realizedGains;
  const thisYear = new Date().getFullYear();

  let yearSel = $state(String(thisYear));
  let year = $derived(Number(yearSel));

  useUrlParams([
    yearParam(
      'year',
      () => yearSel,
      (v) => (yearSel = v)
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => fetchRealizedGains(year),
    label: 'We couldn’t load your realized gains'
  });

  // Years with activity, always reaching the current year.
  let bounds = $derived.by(() => {
    const years = res.data?.years ?? [];
    return {
      min: Math.min(year, thisYear, ...years),
      max: Math.max(year, thisYear, ...years)
    };
  });
  let yearOptions = $derived(buildYearOptions(bounds.min, bounds.max));
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader title={meta.title} description={meta.description}>
      <BudgetYearStepper
        bind:yearSel
        {yearOptions}
        selectId="gains-year"
        atMin={year <= bounds.min}
        atMax={year >= bounds.max}
        onPrev={() => (yearSel = String(Math.max(bounds.min, year - 1)))}
        onNext={() => (yearSel = String(Math.min(bounds.max, year + 1)))}
      />
      <OverflowMenu
        actions={[
          {
            label: 'Export CSV',
            icon: Download,
            onSelect: () => void exportRealizedGainsCsvViaDialog(year)
          }
        ]}
      />
    </PageHeader>

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <KpiCardRow count={4} />
      <TableSkeleton cols={7} rows={6} />
    {:else if res.data}
      {@const r = res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      <DimWrapper active={res.loading} class="flex flex-col gap-6">
        <div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
          <KpiCard title="Net realized" valueClass={signColor(r.netGain)}>
            {#snippet icon()}
              <HandCoins aria-hidden="true" class="size-4 text-muted-foreground" />
            {/snippet}
            {#snippet value()}
              {fmt(r.netGain)}
            {/snippet}
          </KpiCard>
          <KpiCard title="Gains" value={fmt(r.totalGains)}>
            {#snippet icon()}
              <TrendingUp aria-hidden="true" class="size-4 text-success" />
            {/snippet}
          </KpiCard>
          <KpiCard title="Losses" value={fmt(r.totalLosses)}>
            {#snippet icon()}
              <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
            {/snippet}
          </KpiCard>
          <KpiCard title="Dividends in {r.year}" value={fmt(r.dividends)}>
            {#snippet icon()}
              <Coins aria-hidden="true" class="size-4 text-muted-foreground" />
            {/snippet}
          </KpiCard>
        </div>

        {#if r.rows.length === 0}
          <EmptyState
            card
            title="No sales in {r.year}"
            description="Nothing sold this year, so nothing to report. Enjoy the quiet."
          >
            {#snippet icon()}
              <HandCoins aria-hidden="true" class="text-muted-foreground" />
            {/snippet}
          </EmptyState>
        {:else}
          <RealizedGainsTable
            rows={r.rows}
            totalProceeds={r.totalProceeds}
            totalBasis={r.totalBasis}
            netGain={r.netGain}
          />
        {/if}
      </DimWrapper>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
