<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ImportButton from '$lib/components/shared/import-button.svelte';
  import { resolve } from '$app/paths';
  import { Upload, RefreshCw, FileUp, LoaderCircle } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { dbSession, setDbSession } from '$lib/stores/db.svelte';
  import { reimportLast } from '$lib/tauri';
  import { toastPriceResult } from '$lib/features/portfolio/price-utils';
  import { errMsg } from '$lib/utils/error';

  let { onImported }: { onImported?: () => void } = $props();

  let reimporting = $state(false);

  async function handleReimport() {
    if (reimporting || !dbSession.value) return;
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
      onImported?.();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Reimport failed'));
    } finally {
      reimporting = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Data import"
    description="Merge GnuCash files or bank CSV exports into this book."
  >
    {#snippet icon()}
      <Upload aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">GnuCash file</span>
        <span class="text-xs text-muted-foreground">
          Merge accounts, transactions and splits from a .gnucash file
        </span>
      </div>
      <ImportButton label="Import..." variant="outline" disabled={!dbSession.value} {onImported} />
    </div>
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">Bank CSV</span>
        <span class="text-xs text-muted-foreground">
          Map columns and dry-run a preview before importing
        </span>
      </div>
      <Button variant="outline" href={resolve('/import/csv')} disabled={!dbSession.value}>
        <FileUp aria-hidden="true" class="size-4" />
        Import CSV...
      </Button>
    </div>
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">Re-import last file</span>
        <span class="text-xs text-muted-foreground">
          Re-run the most recent GnuCash import if its source file still exists
        </span>
      </div>
      <Button variant="outline" disabled={!dbSession.value || reimporting} onclick={handleReimport}>
        {#if reimporting}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Reimporting...
        {:else}
          <RefreshCw aria-hidden="true" class="size-4" />
          Reimport
        {/if}
      </Button>
    </div>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">No book yet — open or create one from Welcome.</p>
    {/if}
  </Card.Content>
</Card.Root>
