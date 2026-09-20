<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { ArrowLeft, Receipt } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchTransaction } from '$lib/tauri';
  import TransactionForm from '$lib/features/transactions/transaction-form.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  let guid = $derived(page.params.guid);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    fetch: () => {
      const id = guid;
      if (!id) return Promise.reject(new Error('No transaction selected'));
      return fetchTransaction(id);
    },
    label: 'Failed to load transaction'
  });

  let detail = $derived(res.data?.transaction ?? null);
  let detailPath = $derived(detail ? resolve('/transactions/[guid]', { guid: detail.guid }) : null);

  function goBack() {
    if (detail) {
      void goto(resolve('/transactions/[guid]', { guid: detail.guid }), { replaceState: true });
      return;
    }
    void goto(resolve('/transactions'));
  }

  function handleSaved() {
    toast.success('Transaction updated');
    goBack();
  }

  function handleDeleted() {
    toast.success('Transaction deleted');
    void goto(resolve('/transactions'));
  }
</script>

<svelte:head>
  <title>
    {detail ? `Edit ${detail.description || 'transaction'} — Spendo` : 'Edit transaction — Spendo'}
  </title>
  <meta name="description" content="Edit transaction details and splits." />
</svelte:head>

<BookGate title="Edit transaction" description="Edit transaction details and splits">
  <div class="mx-auto flex w-full max-w-6xl flex-col gap-6">
    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="form" />
    {:else if detail}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      <PageHeader title="Edit transaction" description={detail.description || 'Transaction'}>
        <Button variant="outline" href={detailPath ?? resolve('/transactions')}>
          <ArrowLeft aria-hidden="true" class="size-4" />
          Back
        </Button>
      </PageHeader>
      {#key detail.guid}
        <TransactionForm
          mode="edit"
          guid={detail.guid}
          initial={detail}
          onSaved={handleSaved}
          onDeleted={handleDeleted}
          onCancel={goBack}
          onError={(msg) => toast.error(msg)}
        />
      {/key}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else}
      <EmptyState
        card
        title="Transaction not found"
        description="No transaction exists with this ID. It may have been removed by a re-import."
      >
        {#snippet icon()}
          <Receipt aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
        {#snippet action()}
          <Button variant="outline" href={resolve('/transactions')}>
            <ArrowLeft aria-hidden="true" class="size-3" />
            Back to transactions
          </Button>
        {/snippet}
      </EmptyState>
    {/if}
  </div>
</BookGate>
