<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import TableShell from '$lib/components/shared/table-shell.svelte';

  let {
    rows = 8,
    cols = 3
  }: {
    rows?: number;
    cols?: number;
  } = $props();
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      {#each Array(cols) as _, i (i)}
        <Table.Head><Skeleton class="h-4 w-16" /></Table.Head>
      {/each}
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each Array(rows) as _, i (i)}
      <Table.Row>
        {#each Array(cols) as _, j (j)}
          <Table.Cell><Skeleton class="h-4 w-20" /></Table.Cell>
        {/each}
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
