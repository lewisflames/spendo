<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Wallet, RefreshCw, LoaderCircle, Calendar, Building2, Receipt } from '@lucide/svelte';
  import { dbSession, dbSessionLoading, ensureDbSessionLoaded } from '$lib/stores/db.svelte';
  import { reimportLast } from '$lib/tauri';
  import { formatDateRange } from '$lib/utils/format.svelte';
  import ImportHistory from '$lib/features/home/import-history.svelte';
  import ImportButton from '$lib/components/shared/import-button.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageHeaderSkeleton from '$lib/components/shared/page-header-skeleton.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import ErrorCard from '$lib/components/shared/error-card.svelte';

  let importing = $state(false);
  let reimporting = $state(false);
  let importError = $state<string | null>(null);

  $effect(() => {
    // Refresh the shared session on every visit to the home page; failures
    // surface through the import error card / empty state, not a rejection.
    ensureDbSessionLoaded(true).catch(() => {});
  });

  async function handleReimport() {
    importError = null;
    reimporting = true;
    try {
      const result = await reimportLast();
      dbSession.value = { summary: result.summary };
      if (
        result.accountsAdded ||
        result.transactionsAdded ||
        result.splitsAdded ||
        result.commoditiesAdded ||
        result.accountsUpdated ||
        result.transactionsUpdated ||
        result.splitsUpdated
      ) {
        toast.success('Reimport complete');
        toast.info(
          'Prices are not imported — refresh prices in Portfolio to load them from Yahoo Finance.'
        );
      } else {
        toast.warning('No new data imported');
      }
    } catch (e: unknown) {
      importError = errMsg(e, 'Reimport failed');
    } finally {
      reimporting = false;
    }
  }
</script>

<svelte:head>
  <title>Spendo</title>
  <meta name="description" content="Spendo — expense tracking app" />
</svelte:head>

{#if dbSessionLoading.value && !dbSession.value}
  <!-- ── Bootstrapping skeleton (matches the dashboard shape) ── -->
  <section class="flex flex-col gap-6">
    <PageHeaderSkeleton />
    <KpiCardRow />
    <TableSkeleton cols={5} />
  </section>
{:else if dbSession.value}
  <!-- ── Data dashboard ── -->
  <section class="flex flex-col gap-6">
    <PageHeader title="Overview" description="Financial overview and key metrics">
      <ImportButton
        onError={(msg) => (importError = msg)}
        onStatusChange={(v) => (importing = v)}
      />
      <Button variant="outline" onclick={handleReimport} disabled={importing || reimporting}>
        {#if reimporting}
          <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
        {:else}
          <RefreshCw aria-hidden="true" class="size-3" />
        {/if}
        Reimport
      </Button>
    </PageHeader>

    <!-- Dashboard stat cards -->
    <div class="grid gap-6 sm:grid-cols-3">
      <KpiCard title="Accounts" value={String(dbSession.value.summary.totalAccounts)}>
        {#snippet icon()}
          <Building2 aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard title="Transactions" value={String(dbSession.value.summary.totalTransactions)}>
        {#snippet icon()}
          <Receipt aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Date Range"
        value={formatDateRange(
          dbSession.value.summary.earliestDate,
          dbSession.value.summary.latestDate
        )}
        valueClass="text-lg"
      >
        {#snippet icon()}
          <Calendar aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
    </div>

    <!-- Import History -->
    <div class="flex flex-col gap-6">
      {#if importError}
        <ErrorCard message={importError} />
      {/if}
      <ImportHistory />
    </div>
  </section>
{:else}
  <!-- ── Empty state ── -->
  <section class="flex flex-col items-center justify-center gap-8 py-36">
    <div class="flex flex-col items-center gap-6 text-center">
      <div class="flex size-20 items-center justify-center rounded-2xl bg-primary/8">
        <Wallet aria-hidden="true" class="size-10 text-primary" />
      </div>
      <div class="flex flex-col gap-2">
        <h1 class="text-4xl font-bold tracking-tight sm:text-5xl">Spendo</h1>
        <p class="text-lg text-muted-foreground">
          Import your GnuCash file and start exploring your finances.
        </p>
      </div>
    </div>

    {#if importError}
      <ErrorCard message={importError} />
    {/if}

    <ImportButton
      size="lg"
      label="Import GnuCash File"
      onError={(msg) => (importError = msg)}
      onStatusChange={(v) => (importing = v)}
    />
  </section>
{/if}
