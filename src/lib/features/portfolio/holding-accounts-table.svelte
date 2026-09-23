<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import { Briefcase } from '@lucide/svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import {
    signColor,
    formatSignedCurrency,
    formatSessionCurrency as fmt,
    formatShares
  } from '$lib/utils/format.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openAccount } from '$lib/utils/navigation';
  import type { PortfolioHolding } from '$lib/tauri';

  let {
    holding: h,
    currency
  }: {
    holding: PortfolioHolding;
    currency: string;
  } = $props();
</script>

<!-- Accounts holding this security -->
{#if h.accounts.length > 0}
  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <Table.Head class="text-muted-foreground">Account</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Avg Cost</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Cost Basis</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Market Value</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Gain / Loss</Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each h.accounts as acct (acct.accountGuid)}
        {@const unitPrice = h.shares > 0 && h.price !== null ? h.price : 0}
        {@const gainPerShare = h.shares > 0 ? h.gainLoss / h.shares : 0}
        {@const acctGain = acct.shares * gainPerShare}
        <NavRow onNavigate={() => openAccount(acct.accountGuid)}>
          <Table.Cell class="py-2.5">
            <a
              href={resolve('/accounts/[guid]', { guid: acct.accountGuid })}
              class="font-medium focus-visible:outline-2 focus-visible:outline-ring"
              title={acct.accountName}
            >
              {acct.accountDescription || acct.accountName}
            </a>
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {formatShares(acct.shares)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right text-muted-foreground tabular-nums">
            {#if acct.shares > 0}
              {fmt(acct.costBasis / acct.shares)}
            {:else}
              <EmptyValue />
            {/if}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right text-muted-foreground tabular-nums">
            {fmt(acct.costBasis)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right font-medium tabular-nums">
            {fmt(acct.shares * unitPrice)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums {signColor(acctGain)}">
            {formatSignedCurrency(acctGain, currency)}
          </Table.Cell>
        </NavRow>
      {/each}
    </Table.Body>
  </TableShell>
{:else}
  <EmptyState
    card
    title="No accounts hold this security"
    description="The price history is tracked without an open position in your book."
  >
    {#snippet icon()}
      <Briefcase aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{/if}
