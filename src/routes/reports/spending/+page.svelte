<script lang="ts">
  import { resolve } from '$app/paths';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchSpendingReport, type SpendingCategory } from '$lib/tauri';
  import {
    EMPTY_DASH,
    fmtPct,
    formatDateRange,
    formatSessionCurrency as fmt,
    formatSignedSessionCurrency,
    signColor
  } from '$lib/utils/format.svelte';
  import { CalendarRange, ChartColumnStacked, History, Percent, Wallet } from '@lucide/svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { Button } from '$lib/components/ui/button';
  import { ReportPeriodPicker as DateRangePicker } from '$lib/components/shared/report-period-picker';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import SpendingChart from '$lib/features/spending/spending-chart.svelte';
  import SpendingTable from '$lib/features/spending/spending-table.svelte';
  import { buildSpendingSeries, seriesColorByGuid } from '$lib/features/spending/spending-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useBookDefaultPeriod } from '$lib/hooks/use-book-default-period.svelte';
  import { useReportDateRange } from '$lib/hooks/use-report-date-range.svelte';
  import { useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { computePrevRange, PNL_PRESETS } from '$lib/utils/date-utils.svelte';

  const meta = PAGE_META.spending;

  // Period and category come from the URL before the first fetch.
  const range = useReportDateRange();
  useBookDefaultPeriod(range);

  let category = $state('');
  useUrlParams([
    {
      key: 'category',
      get: () => category || null,
      apply: (v) => (category = v ?? '')
    }
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => {
      const { prevFrom, prevTo } = computePrevRange(range.dateFrom, range.dateTo);
      return fetchSpendingReport({
        dateFrom: range.dateFrom,
        dateTo: range.dateTo,
        prevDateFrom: prevFrom,
        prevDateTo: prevTo,
        ...(category ? { parentGuid: category } : {})
      });
    },
    label: 'We couldn’t load your spending'
  });

  let report = $derived(res.data);
  let series = $derived(report ? buildSpendingSeries(report.categories) : []);
  let colors = $derived(seriesColorByGuid(series));
  let loadingFirst = $derived(!report && (res.loading || dbBootstrapping.value));

  /** The current parent is the last crumb; the ones above it link back up. */
  let current = $derived(report?.path.at(-1) ?? null);
  let title = $derived(category && current ? current.accountName : meta.title);

  /** A resolved route plus the current period (and optional extra params). */
  function withPeriod(base: string, extra: Record<string, string> = {}): string {
    const entries = { ...extra, from: range.dateFrom, to: range.dateTo };
    const query = Object.entries(entries)
      .filter(([, v]) => v)
      .map(([k, v]) => `${k}=${encodeURIComponent(v)}`)
      .join('&');
    return query ? `${base}?${query}` : base;
  }

  function reportHref(parentGuid: string | null): string {
    return withPeriod(resolve('/reports/spending'), parentGuid ? { category: parentGuid } : {});
  }

  function hrefFor(c: SpendingCategory): string {
    return c.hasChildren
      ? reportHref(c.accountGuid)
      : withPeriod(resolve('/accounts/[guid]', { guid: c.accountGuid }));
  }

  let breadcrumbs = $derived(
    category && report
      ? [
          { label: meta.title, href: reportHref(null) },
          ...report.path
            .slice(0, -1)
            .map((p) => ({ label: p.accountName, href: reportHref(p.accountGuid) }))
        ]
      : []
  );

  let monthlyAverage = $derived(
    report && report.months.length > 0 ? report.total / report.months.length : null
  );
  let variance = $derived(report ? report.total - report.totalPrev : null);
</script>

<svelte:head>
  <title>{title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader {title} description={meta.description} {breadcrumbs}>
      <DateRangePicker
        bind:value={range.value}
        presets={PNL_PRESETS}
        defaultLabel="Select period"
      />
    </PageHeader>

    {#if !range.rangeValid}
      <ErrorCard
        message="Start date must not be after end date. Adjust the period to see your spending."
      />
    {:else if res.errorKind === 'notFound'}
      <EmptyState
        card
        title="That category is gone"
        description="It may have been deleted or replaced by a re-import."
      >
        {#snippet icon()}
          <ChartColumnStacked aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
        {#snippet action()}
          <Button variant="outline" size="sm" href={reportHref(null)}>All spending</Button>
        {/snippet}
      </EmptyState>
    {:else if loadingFirst}
      <KpiCardRow count={4} />
      <SpendingChart report={null} series={[]} loading />
      <TableSkeleton cols={5} rows={6} />
    {:else if report}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      <DimWrapper active={res.loading} class="flex flex-col gap-6">
        <div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
          <KpiCard title="Spent" value={fmt(report.total)}>
            {#snippet icon()}
              <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
            {/snippet}
          </KpiCard>
          <KpiCard
            title="Monthly average"
            value={monthlyAverage !== null ? fmt(monthlyAverage) : EMPTY_DASH}
          >
            {#snippet icon()}
              <CalendarRange aria-hidden="true" class="size-4 text-muted-foreground" />
            {/snippet}
          </KpiCard>
          <KpiCard title="Previous period" value={fmt(report.totalPrev)}>
            {#snippet icon()}
              <History aria-hidden="true" class="size-4 text-muted-foreground" />
            {/snippet}
          </KpiCard>
          <KpiCard title="Change">
            {#snippet icon()}
              <Percent aria-hidden="true" class="size-4 text-muted-foreground" />
            {/snippet}
            {#snippet value()}
              {#if report.changePct === null || report.changePct === 0}
                <span class="text-muted-foreground">{EMPTY_DASH}</span>
              {:else}
                <span class={signColor(report.changePct, false)}>{fmtPct(report.changePct)}</span>
                <span class="block text-xs font-normal text-muted-foreground"
                  >{formatSignedSessionCurrency(variance ?? 0)} vs previous</span
                >
              {/if}
            {/snippet}
          </KpiCard>
        </div>

        <MissingFxWarning missing={report.missingFx} reportCurrency={report.reportCurrency} />

        {#if report.categories.length === 0}
          <EmptyState
            card
            title="Nothing spent here"
            description="No expenses in this period or the one before. Try a wider range."
          >
            {#snippet icon()}
              <ChartColumnStacked aria-hidden="true" class="text-muted-foreground" />
            {/snippet}
          </EmptyState>
        {:else}
          <SpendingChart {report} {series} loading={false} />
          <SpendingTable
            categories={report.categories}
            {colors}
            total={report.total}
            totalPrev={report.totalPrev}
            changePct={report.changePct}
            {hrefFor}
          />
          <p class="text-xs text-muted-foreground">
            Compared with {formatDateRange(report.prevDateFrom, report.prevDateTo)}. Amounts in
            {report.reportCurrency || 'report currency'} at month-end rates; refunds lower a category’s
            total.
          </p>
        {/if}
      </DimWrapper>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
