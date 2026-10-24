<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { toast } from '$lib/components/ui/sonner';
  import TransactionForm from '$lib/features/transactions/transaction-form.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchHoldingDetail, fetchTransaction } from '$lib/tauri';
  import { openTransaction } from '$lib/utils/navigation';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  let duplicateGuid = $derived(page.url.searchParams.get('duplicate'));
  /** Holding (commodity GUID) to record a dividend for. */
  let dividendGuid = $derived(duplicateGuid ? null : page.url.searchParams.get('dividend'));

  const dupRes = useAsyncData({
    enabled: () => !!dbSession.value && !!duplicateGuid,
    fetch: () => {
      const id = duplicateGuid;
      if (!id) return Promise.reject(new Error('No transaction selected'));
      return fetchTransaction(id);
    },
    label: 'We couldn’t load this transaction'
  });

  // Duplicates start from today’s date with no attachments; the user edits
  // before anything is created.
  let template = $derived(
    dupRes.data?.transaction
      ? { ...dupRes.data.transaction, postDate: todayStr(), attachmentCount: 0 }
      : null
  );

  const dividendRes = useAsyncData({
    enabled: () => !!dbSession.value && !!dividendGuid,
    fetch: () => {
      const g = dividendGuid;
      if (!g) return Promise.reject(new Error('No holding selected'));
      return fetchHoldingDetail(g, 1, 1);
    },
    label: 'We couldn’t load this holding'
  });

  // GnuCash's dividend shape: a zero split on the holding's account (largest
  // position first) ties the payment to it; the income side defaults to the
  // first linked dividend account.
  let dividendPrefill = $derived.by(() => {
    const d = dividendRes.data;
    if (!d) return null;
    const account = [...d.holding.accounts].sort((a, b) => b.shares - a.shares)[0];
    return {
      mnemonic: d.holding.mnemonic,
      description: `${d.holding.mnemonic} dividend`,
      splits: [
        ...(account ? [{ accountGuid: account.accountGuid, amount: 0, memo: 'Dividend' }] : []),
        { accountGuid: d.dividendAccounts[0]?.accountGuid ?? '' },
        { accountGuid: '' }
      ]
    };
  });

  function handleSaved(guid: string) {
    toast.success('Transaction created');
    void openTransaction(guid, { replaceState: true });
  }

  function handleSavedAndNew() {
    toast.success('Transaction created');
  }
</script>

<svelte:head>
  <title
    >{duplicateGuid
      ? 'Duplicate transaction'
      : dividendGuid
        ? 'Record dividend'
        : 'New transaction'} · Spendo</title
  >
  <meta name="description" content="Create a transaction with splits." />
</svelte:head>

<BookGate
  title={duplicateGuid ? 'Duplicate transaction' : 'New transaction'}
  description={duplicateGuid
    ? 'Copy a transaction and its splits'
    : 'Record a transaction with splits'}
>
  <div class="page-form page-stack">
    {#if duplicateGuid}
      {#if dupRes.loading && !dupRes.data}
        <PageSkeleton variant="form" />
      {:else if template}
        {#if dupRes.error}
          <ErrorCard message={dupRes.error} onRetry={() => dupRes.reload()} />
        {/if}
        {#key template.guid}
          <TransactionForm
            mode="create"
            title="Duplicate transaction"
            headerDescription="Both sides must balance."
            breadcrumbs={[{ label: PAGE_META.transactions.title, href: resolve('/transactions') }]}
            saveLabel="Create duplicate"
            initial={template}
            onSaved={handleSaved}
            onSavedAndNew={handleSavedAndNew}
            onCancel={() => void goto(resolve('/transactions'))}
          />
        {/key}
      {:else}
        {#if dupRes.error}
          <ErrorCard message={dupRes.error} onRetry={() => dupRes.reload()} />
        {:else}
          <PageSkeleton variant="form" />
        {/if}
      {/if}
    {:else if dividendGuid}
      {#if dividendPrefill}
        {@const prefill = dividendPrefill}
        {#if dividendRes.error}
          <ErrorCard message={dividendRes.error} onRetry={() => dividendRes.reload()} />
        {/if}
        <TransactionForm
          mode="create"
          title="Record dividend"
          headerDescription="Keep the zero split on {prefill.mnemonic} so the payment counts as its dividend."
          breadcrumbs={[
            { label: PAGE_META.portfolio.title, href: resolve('/portfolio') },
            {
              label: prefill.mnemonic,
              href: resolve('/portfolio/[holding]', { holding: dividendGuid })
            }
          ]}
          {prefill}
          onSaved={handleSaved}
          onCancel={() =>
            void goto(resolve('/portfolio/[holding]', { holding: dividendGuid ?? '' }))}
        />
      {:else if dividendRes.error}
        <ErrorCard message={dividendRes.error} onRetry={() => dividendRes.reload()} />
      {:else}
        <PageSkeleton variant="form" />
      {/if}
    {:else}
      <TransactionForm
        mode="create"
        title="New transaction"
        headerDescription="Both sides must balance."
        breadcrumbs={[{ label: PAGE_META.transactions.title, href: resolve('/transactions') }]}
        onSaved={handleSaved}
        onSavedAndNew={handleSavedAndNew}
        onCancel={() => void goto(resolve('/transactions'))}
      />
    {/if}
  </div>
</BookGate>
