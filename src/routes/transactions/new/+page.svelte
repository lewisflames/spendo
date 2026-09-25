<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { toast } from '$lib/components/ui/sonner';
  import TransactionForm from '$lib/features/transactions/transaction-form.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchTransaction } from '$lib/tauri';
  import { openTransaction } from '$lib/utils/navigation';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  let duplicateGuid = $derived(page.url.searchParams.get('duplicate'));

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

  function handleSaved(guid: string) {
    toast.success('Transaction created');
    void openTransaction(guid, { replaceState: true });
  }

  function handleSavedAndNew() {
    toast.success('Transaction created');
  }
</script>

<svelte:head>
  <title>{duplicateGuid ? 'Duplicate transaction' : 'New transaction'} · Spendo</title>
  <meta name="description" content="Create a transaction with splits." />
</svelte:head>

<BookGate
  title={duplicateGuid ? 'Duplicate transaction' : 'New transaction'}
  description={duplicateGuid
    ? 'Duplicate a transaction with splits'
    : 'Create a transaction with splits'}
>
  <div class="mx-auto flex w-full max-w-5xl flex-col gap-6">
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
            headerDescription="Both sides should balance."
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
        {/if}
        <TransactionForm
          mode="create"
          title="Duplicate transaction"
          headerDescription="Both sides should balance."
          saveLabel="Create duplicate"
          onSaved={handleSaved}
          onSavedAndNew={handleSavedAndNew}
          onCancel={() => void goto(resolve('/transactions'))}
        />
      {/if}
    {:else}
      <TransactionForm
        mode="create"
        title="New transaction"
        headerDescription="Both sides should balance."
        onSaved={handleSaved}
        onSavedAndNew={handleSavedAndNew}
        onCancel={() => void goto(resolve('/transactions'))}
      />
    {/if}
  </div>
</BookGate>
