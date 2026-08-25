<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import {
    ArrowLeft,
    ArrowLeftRight,
    ArrowRight,
    CircleCheck,
    Copy,
    Receipt,
    StickyNote
  } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { dbSession } from '$lib/stores/db.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { fetchTransaction } from '$lib/tauri';
  import type { SplitDetail } from '$lib/tauri';
  import {
    clampZero,
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
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { openTransaction } from '$lib/utils/navigation';

  let guid = $derived(page.params.id);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    fetch: () => {
      const id = guid;
      if (!id) return Promise.reject(new Error('No transaction selected'));
      return fetchTransaction(id);
    },
    label: 'Failed to load transaction'
  });

  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let detail = $derived(res.data?.transaction ?? null);
  let prevGuid = $derived(res.data?.prevGuid ?? null);
  let nextGuid = $derived(res.data?.nextGuid ?? null);
  let currency = $derived(sessionCurrency());

  let reconciledCount = $derived(detail ? detail.splits.filter((s) => s.reconciled).length : 0);

  $effect(() => {
    setBreadcrumb([
      { name: 'Transactions', href: resolve('/transactions') },
      { name: detail?.description || 'Transaction' }
    ]);
  });

  async function copyGuid() {
    if (!detail) return;
    try {
      await navigator.clipboard.writeText(detail.guid);
      toast.success('GUID copied to clipboard');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to copy GUID'));
    }
  }

  /**
   * Native-unit line for a split ("10 AAPL", "-50 USD"): the raw quantity
   * differs from the signed report-currency amount whenever the account's
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
    {detail ? `${detail.description || 'Transaction'} — Spendo` : 'Transaction — Spendo'}
  </title>
  <meta name="description" content="Transaction details and splits." />
</svelte:head>

{#if dbSession.value}
  <div class="flex flex-col gap-6">
    {#if loading && !detail}
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col gap-1">
          <Skeleton class="h-8 w-64" />
          <Skeleton class="h-4 w-40" />
        </div>
      </div>
      <KpiCardRow />
      <TableSkeleton cols={4} rows={3} />
    {:else if error}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {:else if !detail}
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
    {:else if detail}
      {@const d = detail}
      <PageHeader title={d.description || 'Transaction'}>
        {#snippet description()}
          {fmtDate(d.postDate)}{#if d.num}&nbsp;&middot; #{d.num}{/if}
        {/snippet}
        <div class="flex items-center gap-2">
          <div class="flex items-center gap-1">
            <Button
              variant="outline"
              size="icon"
              disabled={!prevGuid || loading}
              aria-label="Previous transaction"
              title="Previous transaction"
              onclick={() => {
                if (prevGuid) void openTransaction(prevGuid);
              }}
            >
              <ArrowLeft aria-hidden="true" />
            </Button>
            <Button
              variant="outline"
              size="icon"
              disabled={!nextGuid || loading}
              aria-label="Next transaction"
              title="Next transaction"
              onclick={() => {
                if (nextGuid) void openTransaction(nextGuid);
              }}
            >
              <ArrowRight aria-hidden="true" />
            </Button>
          </div>
          <Button variant="outline" onclick={copyGuid}>
            <Copy aria-hidden="true" class="size-3" />
            Copy GUID
          </Button>
        </div>
      </PageHeader>

      <div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-3">
        <KpiCard
          title="Total Amount"
          value={fmt(clampZero(d.totalAmount))}
          valueClass={signColor(d.totalAmount)}
        >
          {#snippet icon()}
            <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Reconciled"
          value="{reconciledCount}/{d.splits.length}"
          valueClass={d.splits.length > 0 && reconciledCount === d.splits.length
            ? 'text-success'
            : ''}
        >
          {#snippet icon()}
            <CircleCheck aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Notes">
          {#snippet icon()}
            <StickyNote aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
          {#snippet value()}
            <p
              class="text-sm leading-snug font-medium {d.notes
                ? 'line-clamp-2'
                : 'text-muted-foreground'}"
              title={d.notes ?? undefined}
            >
              {d.notes || '\u2014'}
            </p>
          {/snippet}
        </KpiCard>
      </div>

      <!-- Keep the previous table visible (dimmed) during prev/next refetches
           so the page doesn't jump. -->
      <DimWrapper active={loading}>
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
              <Table.Row class="hover:bg-muted/50">
                <Table.Cell class="py-2.5 font-medium">
                  <a
                    href={resolve('/accounts/[guid]', { guid: split.accountGuid })}
                    class="underline-offset-2"
                    title={split.accountName}
                  >
                    {split.accountDescription || split.accountName}
                  </a>
                  <Badge variant="outline" class="ml-1.5 text-[10px] leading-none">
                    {split.accountType}
                  </Badge>
                </Table.Cell>
                <Table.Cell class="max-w-xs truncate py-2.5 text-muted-foreground">
                  {split.memo || '\u2014'}
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
              </Table.Row>
            {/each}
          </Table.Body>
        </TableShell>
      </DimWrapper>
    {/if}
  </div>
{:else}
  <div class="flex flex-col gap-6">
    <PageHeader title="Transaction" />
    <ImportPrompt />
  </div>
{/if}
