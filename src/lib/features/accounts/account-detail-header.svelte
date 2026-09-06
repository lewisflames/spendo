<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Download, LoaderCircle } from '@lucide/svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import { AsOfPicker } from '$lib/components/shared/as-of-picker';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import type { AccountDetailInfo, AccountSummary } from '$lib/tauri';

  let {
    account,
    summary,
    isToday,
    asOf = $bindable(),
    exporting,
    canExport,
    onExport
  }: {
    account: AccountDetailInfo;
    summary: AccountSummary;
    isToday: boolean;
    asOf: string;
    exporting: boolean;
    canExport: boolean;
    onExport: () => void;
  } = $props();
</script>

<PageHeader title={account.description ?? account.name}>
  {#snippet description()}
    {account.accountType}
    {#if account.placeholder}· Placeholder{/if}
    {#if account.descendantCount > 0}
      · {plural(account.descendantCount, 'sub-account')}
    {/if}
    {#if summary.totalTransactions > 0}
      · {plural(summary.totalTransactions, 'txn')}
    {/if}
    {#if !isToday}
      · as of {formatDate(asOf)}
    {/if}
  {/snippet}
  <AsOfPicker bind:value={asOf} />
  <Button variant="outline" onclick={onExport} disabled={!canExport || exporting}>
    {#if exporting}
      <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
    {:else}
      <Download aria-hidden="true" class="size-3" />
    {/if}
    Export
  </Button>
</PageHeader>
