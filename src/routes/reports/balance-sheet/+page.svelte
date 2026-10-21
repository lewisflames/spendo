<script lang="ts">
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchAccounts } from '$lib/tauri';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyBookPrompt from '$lib/components/shared/empty-book-prompt.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import BalanceSheetStatement from '$lib/features/reports/balance-sheet-statement.svelte';
  import { buildBalanceSheet } from '$lib/features/reports/balance-sheet';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatSessionCurrency } from '$lib/utils/format.svelte';

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => fetchAccounts(),
    label: 'We couldn’t load your accounts'
  });

  let sheet = $derived(res.data ? buildBalanceSheet(res.data) : null);
  let balanced = $derived(res.data !== null && Math.abs(res.data.balanceCheck) < 0.005);
</script>

<svelte:head>
  <title>Balance sheet · Spendo</title>
  <meta name="description" content="What you own and owe, in one statement." />
</svelte:head>

<BookGate title="Balance sheet" description="What you own and owe, at a glance">
  <div class="flex flex-col gap-6">
    <PageHeader title="Balance sheet" description="What you own and owe, as of today" />

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="balance-sheet" />
    {:else if res.data && sheet}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}

      <MissingFxWarning
        missing={res.data.missingFx}
        missingPrices={res.data.missingPrices}
        reportCurrency={res.data.reportCurrency}
      />

      {#if res.data.totalAccounts === 0}
        <EmptyBookPrompt />
      {:else}
        <DimWrapper active={res.loading}>
          <BalanceSheetStatement {sheet} />
        </DimWrapper>

        {#if !res.loading && !res.error}
          <p class="text-xs text-muted-foreground">
            {#if balanced}
              Balanced. Everything adds up.
            {:else}
              Off by {formatSessionCurrency(res.data.balanceCheck)}. Usually a missing FX rate, an
              unpriced security, or currency translation in a book without trading accounts.
            {/if}
          </p>
        {/if}
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
