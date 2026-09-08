<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { RefreshCw, LoaderCircle } from '@lucide/svelte';
  import { setDbSession } from '$lib/stores/db.svelte';
  import { reimportLast } from '$lib/tauri';
  import { toastPriceResult } from '$lib/features/portfolio/price-utils';
  import ChangeHistory from '$lib/features/history/change-history.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import ErrorCard from '$lib/components/shared/error-card.svelte';

  let reimporting = $state(false);
  let importError = $state<string | null>(null);

  async function handleReimport() {
    importError = null;
    reimporting = true;
    try {
      const result = await reimportLast();
      setDbSession(result.summary);
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
      } else {
        toast.warning('No new data imported');
      }
      toastPriceResult(result.prices, { fixHint: true });
    } catch (e: unknown) {
      importError = errMsg(e, 'Reimport failed');
    } finally {
      reimporting = false;
    }
  }
</script>

<svelte:head>
  <title>History — Spendo</title>
  <meta name="description" content="Spendo — imports, edits and reverts." />
</svelte:head>

<BookGate title="History" description="Imports, edits and reverts">
  <div class="flex flex-col gap-6">
    <PageHeader title="History" description="Imports, edits and reverts">
      <Button variant="outline" onclick={handleReimport} disabled={reimporting}>
        {#if reimporting}
          <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
        {:else}
          <RefreshCw aria-hidden="true" class="size-3" />
        {/if}
        Reimport
      </Button>
    </PageHeader>

    {#if importError}
      <ErrorCard message={importError} onRetry={() => void handleReimport()} />
    {/if}
    <ChangeHistory />
  </div>
</BookGate>
