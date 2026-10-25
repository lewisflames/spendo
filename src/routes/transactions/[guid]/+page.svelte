<script lang="ts">
  import Amount from '$lib/components/shared/amount.svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import {
    ArrowLeft,
    ChevronLeft,
    ChevronRight,
    Copy,
    CopyPlus,
    Pencil,
    Receipt
  } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchTransaction } from '$lib/tauri';
  import type { SplitDetail } from '$lib/tauri';
  import {
    formatDate as fmtDate,
    formatNumber,
    formatSplitAmount,
    sessionCurrency
  } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { statusFor } from '$lib/utils/reconcile';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import OverflowMenu from '$lib/components/shared/overflow-menu.svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import TransactionAttachments from '$lib/features/transactions/transaction-attachments.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
  import TagBadges from '$lib/features/tags/tag-badges.svelte';
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
  <meta name="description" content="Details and splits" />
</svelte:head>

<BookGate title="Transaction" description="Details and splits">
  <div class="page-stack">
    {#if (res.loading || dbBootstrapping.value) && !detail}
      <PageSkeleton variant="transaction-detail" />
    {:else if detail}
      {@const d = detail}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      <PageHeader
        title={d.description || 'Transaction'}
        breadcrumbs={[{ label: PAGE_META.transactions.title, href: resolve('/transactions') }]}
      >
        {#snippet description()}
          {d.notes ?? ''}
        {/snippet}
        <ButtonGroup.Root>
          <Button
            variant="outline"
            size="icon"
            disabled={!prevGuid || res.loading}
            aria-label="Previous transaction"
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
            onclick={() => {
              if (nextGuid) void openTransaction(nextGuid);
            }}
          >
            <ChevronRight aria-hidden="true" class="size-4" />
          </Button>
        </ButtonGroup.Root>
        <Button href={resolve('/transactions/[guid]/edit', { guid: d.guid })}>
          <Pencil aria-hidden="true" class="size-4" />
          Edit
        </Button>
        <OverflowMenu
          actions={[
            { label: 'Duplicate', icon: CopyPlus, onSelect: duplicateTransaction },
            { label: 'Copy GUID', icon: Copy, onSelect: () => void copyGuid() }
          ]}
        />
      </PageHeader>

      <!-- Facts about the transaction, not metrics: a compact strip instead of KPI cards. -->
      <dl class="flex flex-wrap gap-x-8 gap-y-3 rounded-lg border bg-muted/25 px-4 py-3 text-sm">
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Date</dt>
          <dd class="font-medium">{fmtDate(d.postDate)}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Total</dt>
          <dd><Amount value={d.totalAmount} tone /></dd>
        </div>
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Reconciled</dt>
          <dd class="font-medium tabular-nums">{reconciledCount} of {d.splits.length} splits</dd>
        </div>
        {#if d.num}
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted-foreground">Number</dt>
            <dd class="font-medium tabular-nums">#{d.num}</dd>
          </div>
        {/if}
        {#if d.tags.length > 0}
          <div class="flex flex-col gap-0.5">
            <dt class="text-xs text-muted-foreground">Tags</dt>
            <dd><TagBadges tags={d.tags} /></dd>
          </div>
        {/if}
      </dl>

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
                  <Amount
                    value={split.amount}
                    format={(n) => formatSplitAmount(n, currency)}
                    tone
                  />
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
