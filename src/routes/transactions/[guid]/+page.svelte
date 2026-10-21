<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import {
    ArrowLeft,
    ArrowLeftRight,
    ChevronLeft,
    ChevronRight,
    CircleCheck,
    Copy,
    CopyPlus,
    Pencil,
    Receipt,
    Calendar
  } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchTransaction } from '$lib/tauri';
  import type { SplitDetail } from '$lib/tauri';
  import {
    clampZero,
    EMPTY_DASH,
    formatDate as fmtDate,
    formatNumber,
    formatSessionCurrency as fmt,
    formatSplitAmount,
    sessionCurrency,
    signColor
  } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { statusFor } from '$lib/utils/reconcile';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import TransactionAttachments from '$lib/features/transactions/transaction-attachments.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { openAccount, openTransaction } from '$lib/utils/navigation';

  let guid = $derived(page.params.guid);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    fetch: () => {
      const id = guid;
      if (!id) return Promise.reject(new Error('No transaction selected'));
      return fetchTransaction(id);
    },
    label: 'We couldn’t load this transaction'
  });

  let detail = $derived(res.data?.transaction ?? null);
  let prevGuid = $derived(res.data?.prevGuid ?? null);
  let nextGuid = $derived(res.data?.nextGuid ?? null);
  let currency = $derived(sessionCurrency());

  let reconciledCount = $derived(detail ? detail.splits.filter((s) => s.reconciled).length : 0);

  async function copyGuid() {
    if (!detail) return;
    try {
      await navigator.clipboard.writeText(detail.guid);
      toast.success('GUID copied');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t copy that'));
    }
  }

  function duplicateTransaction() {
    if (!detail) return;
    // Query strings can’t go through `resolve()` — same pattern as budgets.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(resolve('/transactions/new') + `?duplicate=${encodeURIComponent(detail.guid)}`);
  }

  /**
   * Native-unit line for a split ("10 AAPL", "-50 USD"): the raw quantity
   * differs from the signed report-currency amount whenever the account’s
   * commodity is not the report currency unit (shares, FX).
   */
  function quantityLine(split: SplitDetail): string | null {
    if (split.quantity === 0) return null;
    if (Math.abs(Math.abs(split.quantity) - Math.abs(split.amount)) < 1e-6) return null;
    const n = formatNumber(Math.abs(split.quantity));
    return split.commodity ? `${n} ${split.commodity}` : n;
  }
</script>

<svelte:head>
  <title>
    {detail ? `${detail.description || 'Transaction'} · Spendo` : 'Transaction · Spendo'}
  </title>
  <meta name="description" content="Transaction details and splits." />
</svelte:head>

<BookGate title="Transaction" description="Transaction details and splits.">
  <div class="flex flex-col gap-6">
    {#if (res.loading || dbBootstrapping.value) && !detail}
      <PageSkeleton variant="transaction-detail" />
    {:else if detail}
      {@const d = detail}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      <PageHeader title={d.description || 'Transaction'}>
        {#snippet description()}
          {d.notes ?? EMPTY_DASH}{#if d.num}&nbsp;&middot; #{d.num}{/if}
        {/snippet}
        <Button variant="outline" href={resolve('/transactions')}>
          <ArrowLeft aria-hidden="true" class="size-4" />
          Transactions
        </Button>
        <ButtonGroup.Root>
          <Button
            variant="outline"
            size="icon"
            disabled={!prevGuid || res.loading}
            aria-label="Previous transaction"
            title="Previous transaction"
            onclick={() => {
              if (prevGuid) void openTransaction(prevGuid);
            }}
          >
            <ChevronLeft aria-hidden="true" class="size-4" />
          </Button>
          <Button
            variant="outline"
            size="icon"
            disabled={!nextGuid || res.loading}
            aria-label="Next transaction"
            title="Next transaction"
            onclick={() => {
              if (nextGuid) void openTransaction(nextGuid);
            }}
          >
            <ChevronRight aria-hidden="true" class="size-4" />
          </Button>
        </ButtonGroup.Root>
        <Button
          variant="outline"
          size="icon"
          onclick={copyGuid}
          aria-label="Copy GUID"
          title="Copy GUID"
        >
          <Copy aria-hidden="true" class="size-4" />
        </Button>
        <Button variant="outline" onclick={duplicateTransaction} aria-label="Duplicate transaction">
          <CopyPlus aria-hidden="true" class="size-4" />
          Duplicate
        </Button>
        <Button href={resolve('/transactions/[guid]/edit', { guid: d.guid })}>
          <Pencil aria-hidden="true" class="size-4" />
          Edit
        </Button>
      </PageHeader>

      <div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-3">
        <KpiCard title="Date" value={fmtDate(d.postDate)}>
          {#snippet icon()}
            <Calendar aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Total amount"
          value={fmt(clampZero(d.totalAmount))}
          valueClass={signColor(d.totalAmount)}
        >
          {#snippet icon()}
            <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Reconciled" value="{reconciledCount}/{d.splits.length}">
          {#snippet icon()}
            <CircleCheck aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
      </div>

      <!-- Keep the previous table visible (dimmed) during prev/next refetches
           so the page doesn’t jump. -->
      <DimWrapper active={res.loading}>
        <TableShell>
          <Table.Header>
            <Table.Row class="bg-muted/75">
              <Table.Head class="text-muted-foreground">Account</Table.Head>
              <Table.Head class="text-muted-foreground">Memo</Table.Head>
              <Table.Head class="text-muted-foreground">Status</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Amount</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each d.splits as split (split.guid)}
              {@const qline = quantityLine(split)}
              {@const status = statusFor(split.reconcileState)}
              <NavRow onNavigate={() => openAccount(split.accountGuid)}>
                <Table.Cell class="py-2.5 font-medium">
                  <a
                    href={resolve('/accounts/[guid]', { guid: split.accountGuid })}
                    class="inline-flex items-center focus-visible:outline-2 focus-visible:outline-ring"
                  >
                    <AccountLabel
                      name={split.accountName}
                      description={split.accountDescription}
                      code={split.accountCode}
                    />
                  </a>
                </Table.Cell>
                <Table.Cell class="max-w-xs truncate py-2.5 text-muted-foreground">
                  {#if split.memo}
                    {split.memo}
                  {:else}
                    <EmptyValue />
                  {/if}
                </Table.Cell>
                <Table.Cell class="py-2.5">
                  <Badge variant={status.variant}>{status.label}</Badge>
                </Table.Cell>
                <Table.Cell class="py-2.5 text-right">
                  <span class="font-medium tabular-nums {signColor(split.amount)}">
                    {formatSplitAmount(split.amount, currency)}
                  </span>
                  {#if qline}
                    <p class="text-xs text-muted-foreground tabular-nums">{qline}</p>
                  {/if}
                </Table.Cell>
              </NavRow>
            {/each}
          </Table.Body>
        </TableShell>
      </DimWrapper>

      <!-- Remount per transaction so the attachment list reloads on prev/next. -->
      {#key d.guid}
        <TransactionAttachments txGuid={d.guid} />
      {/key}
    {:else if res.errorKind === 'notFound'}
      <EmptyState
        card
        title="That transaction is gone"
        description="It may have been deleted or replaced by a re-import."
      >
        {#snippet icon()}
          <Receipt aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
        {#snippet action()}
          <Button variant="outline" href={resolve('/transactions')}>
            <ArrowLeft aria-hidden="true" class="size-4" />
            Transactions
          </Button>
        {/snippet}
      </EmptyState>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else}
      <EmptyState
        card
        title="That transaction is gone"
        description="It may have been deleted or replaced by a re-import."
      >
        {#snippet icon()}
          <Receipt aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
        {#snippet action()}
          <Button variant="outline" href={resolve('/transactions')}>
            <ArrowLeft aria-hidden="true" class="size-4" />
            Transactions
          </Button>
        {/snippet}
      </EmptyState>
    {/if}
  </div>
</BookGate>
