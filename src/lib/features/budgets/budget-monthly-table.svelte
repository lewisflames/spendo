<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { cn } from '$lib/utils';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import PacingBar from './pacing-bar.svelte';
  import {
    formatSessionCurrency as fmt,
    formatSignedCurrency,
    signColor
  } from '$lib/utils/format.svelte';
  import { monthName } from '$lib/utils/date-utils.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import type { MonthlySlice } from '$lib/tauri';

  let {
    entries,
    elapsedMonths,
    currency
  }: {
    entries: MonthlySlice[];
    elapsedMonths: number;
    currency: string;
  } = $props();
</script>

<!-- Monthly pacing breakdown (read-only) -->
<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="text-muted-foreground">Month</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Spent</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Cumulative</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Expected</Table.Head>
      <Table.Head class="text-right text-muted-foreground">Ahead / Behind</Table.Head>
      <Table.Head class="text-muted-foreground">Pacing</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each entries as e (e.month)}
      {@const isFuture = elapsedMonths < 12 && e.month > elapsedMonths}
      <Table.Row>
        <Table.Cell class="py-2.5 capitalize">
          {monthName(e.month, appSettings.value.locale)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right tabular-nums">
          {#if isFuture}
            <EmptyValue />
          {:else}
            {fmt(e.actualAmount)}
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right tabular-nums">
          {#if isFuture}
            <EmptyValue />
          {:else}
            {fmt(e.cumulativeActual)}
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right tabular-nums">
          {fmt(e.cumulativeExpected)}
        </Table.Cell>
        <Table.Cell class={cn('py-2.5 text-right tabular-nums', !isFuture && signColor(e.delta))}>
          {#if isFuture}
            <EmptyValue />
          {:else}
            {formatSignedCurrency(e.delta, currency)}
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5">
          {#if isFuture}
            <EmptyValue />
          {:else}
            <PacingBar
              pacingPercentage={e.pacingPercentage}
              actualAmount={e.cumulativeActual}
              expectedYtd={e.cumulativeExpected}
              delta={e.delta}
              {fmt}
            />
          {/if}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
