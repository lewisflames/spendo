<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import { formatDate as fmtDate, formatCurrency } from '$lib/format.svelte';
  import type { TransactionRow } from '$lib/server/transactions';

  let {
    transactions = [],
    currency = 'EUR'
  }: {
    transactions?: TransactionRow[];
    currency?: string;
  } = $props();

  function fmtTransaction(n: number): string {
    return formatCurrency(n, currency);
  }
</script>

<div class="overflow-x-auto rounded-lg border">
  <table class="w-full min-w-150">
    <thead>
      <tr class="bg-muted/50 text-left text-sm text-muted-foreground">
        <th class="px-4 py-3 font-medium">Date</th>
        <th class="px-4 py-3 font-medium">Description</th>
        <th class="px-4 py-3 font-medium">Debit</th>
        <th class="px-4 py-3 font-medium">Credit</th>
        <th class="px-4 py-3 text-right font-medium">Amount</th>
      </tr>
    </thead>
    <tbody class="divide-y text-sm">
      {#each transactions as txn (txn.guid)}
        {@const posSplits = txn.splits.filter((s) => s.amount > 0)}
        {@const negSplits = txn.splits.filter((s) => s.amount < 0)}
        <tr class="hover:bg-muted/30 transition-colors">
          <td class="whitespace-nowrap px-4 py-3 text-muted-foreground">
            {fmtDate(txn.postDate)}
          </td>
          <td class="max-w-xs truncate px-4 py-3 font-medium" title={txn.description}>
            {txn.description || '\u2014'}
          </td>
          <td class="px-4 py-3">
            {#each posSplits as split (split.guid)}
              <div class="flex items-center gap-1.5">
                <span class="truncate">{split.accountName}</span>
                <Badge variant="outline" class="shrink-0 text-[10px] leading-none">
                  {split.accountType}
                </Badge>
              </div>
            {/each}
          </td>
          <td class="px-4 py-3">
            {#each negSplits as split (split.guid)}
              <div class="flex items-center gap-1.5">
                <span class="truncate">{split.accountName}</span>
                <Badge variant="outline" class="shrink-0 text-[10px] leading-none">
                  {split.accountType}
                </Badge>
              </div>
            {/each}
          </td>
          <td class="whitespace-nowrap px-4 py-3 text-right font-medium tabular-nums">
            {#each txn.splits as split (split.guid)}
              <div>{fmtTransaction(Math.abs(split.amount))}</div>
            {/each}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
