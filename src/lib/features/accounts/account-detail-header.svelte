<script lang="ts">
  import type { Snippet } from 'svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import { plural } from '$lib/utils/format.svelte';
  import type { AccountDetailInfo, AccountSummary } from '$lib/tauri';

  let {
    account,
    summary,
    children = undefined
  }: {
    account: AccountDetailInfo;
    summary: AccountSummary;
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
      · {plural(summary.totalTransactions, 'transaction')}
    {/if}
  {/snippet}
  {#if children}
    {@render children()}
  {/if}
</PageHeader>
