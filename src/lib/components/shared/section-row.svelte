<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import Amount from '$lib/components/shared/amount.svelte';

  /**
   * Group row (Assets, Liabilities, …) inside a table body: a light band at
   * body text size, so the group reads as part of the table rather than a
   * caption, with its optional total in the last column.
   */
  let {
    label,
    total = undefined,
    columns
  }: {
    label: string;
    total?: number;
    /** Total column count of the table. */
    columns: number;
  } = $props();
</script>

<Table.Row class="border-t bg-muted/40 hover:bg-muted/40">
  <Table.Cell colspan={total === undefined ? columns : columns - 1} class="py-2 font-semibold">
    {label}
  </Table.Cell>
  {#if total !== undefined}
    <Table.Cell class="py-2 text-right">
      <Amount value={total} weight="semibold" />
    </Table.Cell>
  {/if}
</Table.Row>
