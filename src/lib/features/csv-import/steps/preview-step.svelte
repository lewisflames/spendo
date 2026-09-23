<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Table from '$lib/components/ui/table';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { ArrowLeft, LoaderCircle, Sparkles, Upload } from '@lucide/svelte';
  import type { SvelteSet } from 'svelte/reactivity';
  import type { CsvPreviewResult } from '$lib/tauri';
  import { formatCurrency, formatDate, plural, sessionCurrency } from '$lib/utils/format.svelte';

  let {
    preview,
    selected,
    page = $bindable(),
    pageSize = $bindable(),
    commitError,
    committing,
    targetCommodity,
    onCommit,
    onBack
  }: {
    preview: CsvPreviewResult;
    selected: SvelteSet<number>;
    page: number;
    pageSize: number;
    commitError: string | null;
    committing: boolean;
    targetCommodity: string | null;
    onCommit: () => void;
    onBack: () => void;
  } = $props();

  let currency = $derived(targetCommodity ?? sessionCurrency());

  let totalPages = $derived(Math.max(1, Math.ceil(preview.rows.length / pageSize)));
  let pageRows = $derived.by(() => {
    const start = (page - 1) * pageSize;
    return preview.rows.slice(start, start + pageSize);
  });

  let importableKeys = $derived.by(() =>
    preview.rows.filter((r) => r.valid && !r.duplicate).map((r) => r.key)
  );
  let allImportableSelected = $derived.by(
    () => importableKeys.length > 0 && importableKeys.every((k) => selected.has(k))
  );
  const rulesApplied = $derived(
    preview.rows.filter((r) => r.valid && !r.duplicate && r.ruleMatched).length
  );

  function toggleRow(key: number, next: boolean) {
    if (next) selected.add(key);
    else selected.delete(key);
  }

  function toggleAll(next: boolean) {
    selected.clear();
    if (next) {
      for (const k of importableKeys) selected.add(k);
    }
  }
</script>

<div class="flex flex-col gap-4">
  <Card.Root>
    <Card.Header>
      <Card.Title>Dry-run preview</Card.Title>
      <Card.Description>
        {preview.validCount} ready · {preview.duplicateCount} duplicates (pre-unchecked) ·
        {preview.errorCount} errors · {selected.size} selected{#if rulesApplied > 0}
          · {plural(rulesApplied, 'row')} via rules{/if}
      </Card.Description>
    </Card.Header>
    <Card.Content>
      <TableShell>
        <Table.Header>
          <Table.Row class="bg-muted/75">
            <Table.Head class="w-10 text-muted-foreground">
              <Checkbox
                checked={allImportableSelected}
                indeterminate={!allImportableSelected && selected.size > 0}
                disabled={importableKeys.length === 0}
                aria-label={allImportableSelected
                  ? 'Unselect all importable rows'
                  : 'Select all importable rows'}
                onCheckedChange={(v) => toggleAll(v === true)}
              />
            </Table.Head>
            <Table.Head class="text-muted-foreground">Date</Table.Head>
            <Table.Head class="text-muted-foreground">Description</Table.Head>
            <Table.Head class="text-muted-foreground">Category</Table.Head>
            <Table.Head class="text-right text-muted-foreground tabular-nums">Amount</Table.Head>
            <Table.Head class="text-muted-foreground">Status</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each pageRows as row (row.key)}
            {@const checked = selected.has(row.key)}
            {@const disabled = !row.valid || row.duplicate}
            <Table.Row data-checked={checked}>
              <Table.Cell class="py-2.5">
                <Checkbox
                  {checked}
                  {disabled}
                  aria-label="Select row {row.key + 1}"
                  onCheckedChange={(v) => toggleRow(row.key, v === true)}
                />
              </Table.Cell>
              <Table.Cell class="py-2.5 whitespace-nowrap">
                {#if row.postDate}
                  {formatDate(row.postDate)}
                {:else}
                  <EmptyValue />
                {/if}
              </Table.Cell>
              <Table.Cell
                class="max-w-md truncate py-2.5 font-medium"
                title={row.description ?? ''}
              >
                {#if row.description}
                  {row.description}
                {:else}
                  <EmptyValue />
                {/if}
              </Table.Cell>
              <Table.Cell
                class="max-w-40 truncate py-2.5 text-muted-foreground"
                title={row.ruleMatched ? 'Matched an import rule' : 'Fallback category'}
              >
                <span class="flex items-center gap-1">
                  {#if row.ruleMatched}
                    <Sparkles
                      class="size-3 shrink-0 text-muted-foreground"
                      aria-label="Matched an import rule"
                    />
                  {/if}
                  {row.offsetAccountName}
                </span>
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right tabular-nums">
                {#if row.amount !== null}
                  {formatCurrency(row.amount, currency)}
                {:else}
                  <EmptyValue />
                {/if}
              </Table.Cell>
              <Table.Cell class="py-2.5">
                {#if !row.valid}
                  <Badge variant="destructive" title={row.error ?? ''}>Error</Badge>
                {:else if row.duplicate}
                  <Badge variant="secondary">Duplicate</Badge>
                {:else}
                  <Badge variant="outline">Ready</Badge>
                {/if}
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </TableShell>
      <div class="mt-4">
        <PaginationNav
          bind:page
          {totalPages}
          totalItems={preview.rows.length}
          {pageSize}
          onPageSizeChange={(size) => {
            pageSize = size;
            page = 1;
          }}
        />
      </div>
      {#if preview.errorCount > 0}
        <p class="mt-2 text-xs text-muted-foreground">
          Rows with errors stay disabled. Fix the mapping or the file, then re-run preview.
        </p>
      {/if}
    </Card.Content>
  </Card.Root>

  {#if commitError}
    <ErrorCard message={commitError} onRetry={onCommit} />
  {/if}

  <div class="flex items-center gap-2">
    <Button variant="outline" onclick={onBack}>
      <ArrowLeft aria-hidden="true" class="size-4" />
      Mapping
    </Button>
    <Button onclick={onCommit} disabled={committing || selected.size === 0}>
      {#if committing}
        <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
        Importing…
      {:else}
        <Upload aria-hidden="true" class="size-4" />
        Import {plural(selected.size, 'row')}
      {/if}
    </Button>
  </div>
</div>
