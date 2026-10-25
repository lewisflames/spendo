<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchTags, fetchTaxSummary } from '$lib/tauri';
  import {
    BadgePercent,
    Download,
    Landmark,
    ListChecks,
    Percent,
    ReceiptText,
    Wallet
  } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
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
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import TaxStatement from '$lib/features/tax/tax-statement.svelte';
  import TaxGainsCard from '$lib/features/tax/tax-gains-card.svelte';
  import TaxDeductibleCard from '$lib/features/tax/tax-deductible-card.svelte';
  import TaxRolesDialog from '$lib/features/tax/tax-roles-dialog.svelte';
  import { hasTaxData } from '$lib/features/tax/tax-utils';
  import { exportTaxSummaryCsvViaDialog } from '$lib/features/book/import-export-actions';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useUrlParams, yearParam } from '$lib/hooks/use-url-params.svelte';
  import { formatAllocPct, formatSessionCurrency as fmt, plural } from '$lib/utils/format.svelte';

  const meta = PAGE_META.tax;
  const thisYear = new Date().getFullYear();

  // The return you file is for last year, so that's where the page opens.
  let yearSel = $state(String(thisYear - 1));
  let year = $derived(Number(yearSel));
  let rolesOpen = $state(false);

  useUrlParams([
    yearParam(
      'year',
      () => yearSel,
      (v) => (yearSel = v)
    )
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => fetchTaxSummary(year),
    label: 'We couldn’t load your tax summary'
  });

  const tagsRes = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchTags,
    label: 'We couldn’t load your tags'
  });

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
        selectId="tax-year"
        atMin={year <= bounds.min}
        atMax={year >= bounds.max}
        onPrev={() => (yearSel = String(Math.max(bounds.min, year - 1)))}
        onNext={() => (yearSel = String(Math.min(bounds.max, year + 1)))}
      />
      <Button variant="outline" onclick={() => (rolesOpen = true)}>
        <ListChecks aria-hidden="true" class="size-4" />
        Tax roles
      </Button>
      <OverflowMenu
        actions={[
          {
            label: 'Export CSV',
            icon: Download,
            onSelect: () => void exportTaxSummaryCsvViaDialog(year)
          }
        ]}
      />
    </PageHeader>

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <KpiCardRow count={4} />
      <TableSkeleton cols={2} rows={10} />
    {:else if res.data}
      {@const s = res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if !hasTaxData(s)}
        <EmptyState
          card
          title="Nothing to sum up for {s.year} yet"
          description={s.untagged.length > 0
            ? 'Mark which accounts hold wages, interest and the tax taken from them, and the totals show up here.'
            : 'No taxable income recorded this year.'}
        >
          {#snippet icon()}
            <BadgePercent aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
          {#snippet action()}
            <Button onclick={() => (rolesOpen = true)}>
              <ListChecks aria-hidden="true" class="size-4" />
              Set up tax roles
            </Button>
          {/snippet}
        </EmptyState>
      {:else}
        <DimWrapper active={res.loading} class="flex flex-col gap-6">
          <MissingFxWarning missing={s.missingFx} reportCurrency={s.reportCurrency} />
          {#if s.isPartial}
            <p
              role="note"
              class="rounded-lg border bg-muted/25 px-3 py-2 text-sm text-muted-foreground"
            >
              {s.year} isn’t over yet, so these are totals so far.
            </p>
          {/if}
          {#if s.untagged.length > 0}
            <div
              role="note"
              class="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-sm"
            >
              <p class="min-w-0 flex-1">
                {plural(s.untagged.length, 'income account')} with activity in {s.year}
                {s.untagged.length === 1 ? 'has' : 'have'} no tax role:
                {s.untagged.map((l) => l.accountName).join(', ')}.
              </p>
              <Button variant="outline" size="sm" onclick={() => (rolesOpen = true)}>
                Assign roles
              </Button>
            </div>
          {/if}

          <div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
            <KpiCard title="Gross income" value={fmt(s.grossIncome)}>
              {#snippet icon()}
                <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
              {/snippet}
            </KpiCard>
            <KpiCard title="Income tax" value={fmt(s.incomeTax)}>
              {#snippet icon()}
                <Landmark aria-hidden="true" class="size-4 text-muted-foreground" />
              {/snippet}
            </KpiCard>
            <KpiCard
              title="Effective rate"
              value={s.effectiveRate === null ? '—' : formatAllocPct(s.effectiveRate * 100)}
            >
              {#snippet icon()}
                <Percent aria-hidden="true" class="size-4 text-muted-foreground" />
              {/snippet}
            </KpiCard>
            <KpiCard
              title={s.settlement.total < 0 ? 'Return refund' : 'Return payment'}
              value={s.settlement.lines.length === 0
                ? s.isPartial
                  ? 'Not filed yet'
                  : 'None'
                : fmt(Math.abs(s.settlement.total))}
            >
              {#snippet icon()}
                <ReceiptText aria-hidden="true" class="size-4 text-muted-foreground" />
              {/snippet}
            </KpiCard>
          </div>

          <div class="grid grid-cols-1 items-start gap-6 @4xl:grid-cols-[2fr_1fr]">
            <TaxStatement summary={s} />
            <div class="flex flex-col gap-6">
              <TaxGainsCard summary={s} />
              <TaxDeductibleCard
                summary={s}
                tags={tagsRes.data ?? []}
                onChanged={() => res.reload()}
              />
              {#if s.exempt.lines.length > 0}
                <p class="px-1 text-xs text-muted-foreground">
                  Left out as exempt: {s.exempt.lines.map((l) => l.accountName).join(', ')}
                  ({fmt(s.exempt.total)}).
                </p>
              {/if}
            </div>
          </div>
        </DimWrapper>
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>

<TaxRolesDialog bind:open={rolesOpen} onSaved={() => res.reload()} />
