<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Button } from '$lib/components/ui/button';
  import { Upload, FileUp, ChevronDown, LoaderCircle } from '@lucide/svelte';
  import { setDbSession } from '$lib/stores/db.svelte';
  import { importGnucash } from '$lib/tauri';
  import { toastPriceResult } from '$lib/features/portfolio/price-utils';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { open } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';

  let {
    onError,
    onStatusChange,
    onImported
  }: {
    onError?: (msg: string) => void;
    onStatusChange?: (importing: boolean) => void;
    /** Called after a successful GnuCash import (e.g. to reload the ledger). */
    onImported?: () => void;
  } = $props();

  let importing = $state(false);

  function setImporting(value: boolean) {
    importing = value;
    onStatusChange?.(value);
  }

  async function handleGnucashImport() {
    setImporting(true);
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'GnuCash', extensions: ['gnucash'] }]
      });
      if (!selected) return;
      const filename = await basename(selected);
      const result = await importGnucash(selected, filename);
      setDbSession(result.summary);
      toast.success(`Imported ${filename}`);
      toastPriceResult(result.prices, { fixHint: true });
      onImported?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Import failed');
      if (onError) {
        onError(msg);
      } else {
        toast.error(msg);
      }
    } finally {
      setImporting(false);
    }
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button {...props} variant="outline" disabled={importing}>
        {#if importing}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
        {:else}
          <Upload aria-hidden="true" class="size-4" />
        {/if}
        Import
        <ChevronDown aria-hidden="true" class="size-4 text-muted-foreground" />
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content align="end">
    <DropdownMenu.Item
      onclick={() => void goto(resolve('/transactions/import'))}
      disabled={importing}
    >
      <FileUp aria-hidden="true" />
      Import CSV…
    </DropdownMenu.Item>
    <DropdownMenu.Item onclick={() => void handleGnucashImport()} disabled={importing}>
      <Upload aria-hidden="true" />
      Import from GnuCash…
    </DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>
