<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { ArrowLeft } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import TransactionForm from '$lib/features/transactions/transaction-form.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import { openTransaction } from '$lib/utils/navigation';

  $effect(() => {
    setBreadcrumb([
      { name: 'Transactions', href: resolve('/transactions') },
      { name: 'New transaction' }
    ]);
  });

  function handleSaved(guid: string) {
    toast.success('Transaction created');
    void openTransaction(guid, { replaceState: true });
  }
</script>

<svelte:head>
  <title>New transaction — Spendo</title>
  <meta name="description" content="Create a transaction with splits." />
</svelte:head>

<BookGate title="New transaction" description="Create a transaction with splits">
  <div class="mx-auto flex w-full max-w-6xl flex-col gap-6">
    <PageHeader
      title="New transaction"
      description="Splits use raw amounts as shown in the detail view and must sum to zero."
    >
      <Button variant="outline" href={resolve('/transactions')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Transactions
      </Button>
    </PageHeader>
    <TransactionForm
      mode="create"
      onSaved={handleSaved}
      onCancel={() => void goto(resolve('/transactions'))}
      onError={(msg) => toast.error(msg)}
    />
  </div>
</BookGate>
