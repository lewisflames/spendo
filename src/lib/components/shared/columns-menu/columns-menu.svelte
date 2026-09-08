<script lang="ts" generics="Id extends string">
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Button } from '$lib/components/ui/button';
  import { Columns3 } from '@lucide/svelte';
  import type { ColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';

  let {
    columns,
    cols,
    onHide
  }: {
    columns: { id: Id; label: string }[];
    cols: ColumnVisibility<Id>;
    /** Called after a column is hidden (e.g. to reset a sort on that column). */
    onHide?: (id: Id) => void;
  } = $props();

  function handleCheckedChange(id: Id, checked: boolean) {
    cols.setVisible(
      id,
      checked,
      columns.map((c) => c.id)
    );
    if (!checked) onHide?.(id);
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button {...props} variant="outline" size="sm">
        <Columns3 aria-hidden="true" class="text-muted-foreground" />
        Columns
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content align="end">
    {#each columns as col (col.id)}
      <DropdownMenu.CheckboxItem
        checked={cols.isVisible(col.id)}
        onCheckedChange={(checked) => handleCheckedChange(col.id, checked)}
        closeOnSelect={false}
      >
        {col.label}
      </DropdownMenu.CheckboxItem>
    {/each}
    <DropdownMenu.Separator />
    <DropdownMenu.Item onclick={() => cols.resetColumns()}>Reset to default</DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>
