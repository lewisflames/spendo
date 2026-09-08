<script lang="ts">
  import type { Snippet } from 'svelte';
  import * as Table from '$lib/components/ui/table';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';

  let {
    loading,
    error,
    onRetry,
    hasData,
    isEmpty,
    emptyMessage,
    emptyTitle = 'No data',
    skeletonCols,
    skeletonRows = 4,
    header,
    children
  }: {
    loading: boolean;
    error: string | null;
    onRetry?: () => void;
    /** True once the report has loaded (keeps the table mounted + dimmed on refetch). */
    hasData: boolean;
    /** True when the report loaded but has nothing to show. */
    isEmpty: boolean;
    emptyMessage: string;
    emptyTitle?: string;
    skeletonCols: number;
    skeletonRows?: number;
    /** Renders the `<Table.Header>` element. */
    header: Snippet;
    /** Renders the body rows (wrapped in `<Table.Body>`). */
    children: Snippet;
  } = $props();
</script>

{#if loading && !hasData}
  <TableSkeleton cols={skeletonCols} rows={skeletonRows} />
{:else if hasData}
  {#if error}
    <ErrorCard message={error} {onRetry} />
  {/if}
  {#if isEmpty}
    <EmptyState card title={emptyTitle} description={emptyMessage} />
  {:else}
    <!-- Keep the previous table visible (dimmed) during refetches so the
         page doesn't jump and expand/collapse state stays in place. -->
    <DimWrapper active={loading}>
      <TableShell>
        {@render header()}
        <Table.Body>
          {@render children()}
        </Table.Body>
      </TableShell>
    </DimWrapper>
  {/if}
{:else if error}
  <ErrorCard message={error} {onRetry} />
{:else if isEmpty}
  <EmptyState card title={emptyTitle} description={emptyMessage} />
{:else}
  <!-- Keep the previous table visible (dimmed) during refetches so the
       page doesn't jump and expand/collapse state stays in place. -->
  <DimWrapper active={loading}>
    <TableShell>
      {@render header()}
      <Table.Body>
        {@render children()}
      </Table.Body>
    </TableShell>
  </DimWrapper>
{/if}
