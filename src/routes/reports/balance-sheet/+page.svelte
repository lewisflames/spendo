<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchBalanceSheet } from '$lib/tauri';
  import * as Select from '$lib/components/ui/select';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyBookPrompt from '$lib/components/shared/empty-book-prompt.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import BalanceSheetStatement from '$lib/features/reports/balance-sheet-statement.svelte';
  import {
    buildBalanceSheet,
    COMPARISON_OPTIONS,
    comparisonDate,
    isComparison,
    type BalanceSheetComparison
  } from '$lib/features/reports/balance-sheet';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { dateParam, useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { isIsoDate, today } from '$lib/utils/date-utils.svelte';
  import { formatDate, formatSessionCurrency } from '$lib/utils/format.svelte';

  const meta = PAGE_META.balanceSheet;
  const NO_COMPARISON = 'none';

  /** Empty means today, valued exactly like the accounts page. */
  let asOf = $state('');
  let compare = $state<BalanceSheetComparison | typeof NO_COMPARISON>(NO_COMPARISON);

  useUrlParams([
    dateParam(
      'asOf',
      () => asOf || null,
      (v) => (asOf = v && isIsoDate(v) && v <= today.value ? v : '')
    ),
    {
      key: 'compare',
      get: () => (compare === NO_COMPARISON ? null : compare),
      apply: (v) => (compare = isComparison(v) ? v : NO_COMPARISON),
      replace: true
    }
  ]);

  let effectiveAsOf = $derived(asOf || today.value);
  let compareTo = $derived(
    compare === NO_COMPARISON ? null : comparisonDate(effectiveAsOf, compare)
  );

  const compareItems = [
    { value: NO_COMPARISON, label: 'No comparison' },
    ...COMPARISON_OPTIONS.map((o) => ({ value: o.value, label: `vs ${o.label.toLowerCase()}` }))
  ];

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () =>
      fetchBalanceSheet({
        ...(asOf ? { asOf } : {}),
        ...(compareTo ? { compareTo } : {})
      }),
    label: 'We couldn’t load the balance sheet'
  });

  let current = $derived(res.data?.current ?? null);
  let sheet = $derived(res.data ? buildBalanceSheet(res.data.current, res.data.compare) : null);

  /** Missing rates or prices on either date, each listed once. */
  let missingFx = $derived([
    ...new Set([...(current?.missingFx ?? []), ...(res.data?.compare?.missingFx ?? [])])
  ]);
  let missingPrices = $derived.by(() => {
    const all = [...(current?.missingPrices ?? []), ...(res.data?.compare?.missingPrices ?? [])];
    return all.filter((p, i) => all.findIndex((q) => q.mnemonic === p.mnemonic) === i);
  });

  const isBalanced = (check: number) => Math.abs(check) < 0.005;
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader title={meta.title} description={meta.description}>
      <DatePicker bind:value={asOf} placeholder="Today" max={today.value} clearable />
      <Select.Root type="single" bind:value={compare} items={compareItems}>
        <Select.Trigger aria-label="Compare with" class="min-w-44">
          <SelectPrimitive.Value />
        </Select.Trigger>
        <Select.Content>
          {#each compareItems as item (item.value)}
            <Select.Item value={item.value} label={item.label}>{item.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </PageHeader>

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="balance-sheet" />
    {:else if res.data && current && sheet}
      {@const data = res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}

      <MissingFxWarning
        missing={missingFx}
        {missingPrices}
        reportCurrency={current.reportCurrency}
      />

      {#if current.totalAccounts === 0}
        <EmptyBookPrompt />
      {:else}
        <DimWrapper active={res.loading}>
          <BalanceSheetStatement {sheet} asOf={data.asOf} compareTo={data.compareTo} />
        </DimWrapper>

        {#if !res.loading && !res.error}
          {@const offDates = [
            { date: data.asOf, check: current.balanceCheck },
            ...(data.compare && data.compareTo
              ? [{ date: data.compareTo, check: data.compare.balanceCheck }]
              : [])
          ].filter((d) => !isBalanced(d.check))}
          <p class="text-xs text-muted-foreground">
            {#if offDates.length === 0}
              Balanced. Everything adds up.
            {:else}
              {offDates
                .map((d) => `Off by ${formatSessionCurrency(d.check)} on ${formatDate(d.date)}`)
                .join('; ')}. Usually a missing FX rate, an unpriced security, or currency
              translation in a book without trading accounts.
            {/if}
            {#if asOf || data.compareTo}
              Past dates use the last price and FX rate known on that day.
            {/if}
          </p>
        {/if}
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
