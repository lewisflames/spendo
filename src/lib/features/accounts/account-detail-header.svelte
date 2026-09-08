<script lang="ts">
  import type { Snippet } from 'svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import { AsOfPicker } from '$lib/components/shared/as-of-picker';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import type { AccountDetailInfo, AccountSummary } from '$lib/tauri';

  let {
    account,
    summary,
    isToday,
    asOf = $bindable(),
    children = undefined
  }: {
    account: AccountDetailInfo;
    summary: AccountSummary;
    isToday: boolean;
    asOf: string;
    children?: Snippet;
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
  <div class="flex items-center gap-2">
    {#if children}
      {@render children()}
    {/if}
    <AsOfPicker bind:value={asOf} />
  </div>
</PageHeader>
