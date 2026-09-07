<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Download, FileDown, LoaderCircle } from '@lucide/svelte';
  import { save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { dbSession } from '$lib/stores/db.svelte';
  import { writeTextFile } from '$lib/tauri';
  import { ensureCsvExtension } from '$lib/utils/book';
  import { errMsg } from '$lib/utils/error';
  import {
    ACCOUNTS_EXPORT_FILENAME,
    TRANSACTIONS_EXPORT_FILENAME,
    buildAccountsExport,
    buildTransactionsExport
  } from './exporters';

  let exportingTransactions = $state(false);
  let exportingAccounts = $state(false);

  let busy = $derived(exportingTransactions || exportingAccounts);

  async function handleExport(kind: 'transactions' | 'accounts') {
    const isTransactions = kind === 'transactions';
    if (busy) return;
    let target: string | null;
    try {
      target = await save({
        defaultPath: isTransactions ? TRANSACTIONS_EXPORT_FILENAME : ACCOUNTS_EXPORT_FILENAME,
        filters: [{ name: 'CSV', extensions: ['csv'] }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return;
    }
    // Null means the user cancelled — stay silent.
    if (!target) return;
    const file = ensureCsvExtension(target);
    if (isTransactions) exportingTransactions = true;
    else exportingAccounts = true;
    try {
      const contents = isTransactions
        ? await buildTransactionsExport()
        : await buildAccountsExport();
      await writeTextFile(file, contents);
      toast.success(`Saved ${await basename(file)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, `Failed to export ${kind}`));
    } finally {
      if (isTransactions) exportingTransactions = false;
      else exportingAccounts = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader title="Data export" description="Full-book CSV dumps saved via the file dialog.">
    {#snippet icon()}
      <FileDown aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">Transactions</span>
        <span class="text-xs text-muted-foreground"
          >One row per split, newest first — GnuCash multi-split compatible</span
        >
      </div>
      <Button
        variant="outline"
        disabled={!dbSession.value || busy}
        onclick={() => void handleExport('transactions')}
      >
        {#if exportingTransactions}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Exporting...
        {:else}
          <Download aria-hidden="true" class="size-4" />
          Export
        {/if}
      </Button>
    </div>
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium">Accounts</span>
        <span class="text-xs text-muted-foreground"
          >Full account tree with hierarchy and commodities</span
        >
      </div>
      <Button
        variant="outline"
        disabled={!dbSession.value || busy}
        onclick={() => void handleExport('accounts')}
      >
        {#if exportingAccounts}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Exporting...
        {:else}
          <Download aria-hidden="true" class="size-4" />
          Export
        {/if}
      </Button>
    </div>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">No book yet — start from Overview.</p>
    {/if}
  </Card.Content>
</Card.Root>
