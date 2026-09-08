<script lang="ts">
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import { AsOfPicker } from '$lib/components/shared/as-of-picker';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import type { AccountDetailInfo, AccountSummary } from '$lib/tauri';

  let {
    account,
    summary,
    isToday,
    asOf = $bindable()
  }: {
    account: AccountDetailInfo;
    summary: AccountSummary;
    isToday: boolean;
    asOf: string;
  } = $props();
</script>

<PageHeader title={account.description ?? account.name}>
  {#snippet description()}
    {account.accountType}
    {#if account.placeholder}· Placeholder{/if}
    {#if account.descendantsCount > 0}
      · {plural(account.descendantsCount, 'sub-account')}
    {/if}
    {#if summary.totalTransactions > 0}
      · {plural(summary.totalTransactions, 'txn')}
    {/if}
    {#if !isToday}
      · as of {formatDate(asOf)}
    {/if}
  {/snippet}
  <AsOfPicker bind:value={asOf} />
</PageHeader>
