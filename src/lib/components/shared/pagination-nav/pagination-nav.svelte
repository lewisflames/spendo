<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { PAGE_SIZE_OPTIONS } from '$lib/stores/settings.svelte';
  import { formatNumber } from '$lib/utils/format.svelte';
  import { ArrowLeft, ArrowRight } from '@lucide/svelte';

  let {
    page = $bindable(1),
    totalPages = 1,
    pageSize,
    totalItems,
    onPageSizeChange
  }: {
    page?: number;
    totalPages?: number;
    pageSize?: number;
    /** Total row count across all pages; renders "Showing X–Y of Z" instead of "Page X of Y". */
    totalItems?: number;
    onPageSizeChange?: (size: number) => void;
  } = $props();

  const hasPageSize = $derived(pageSize !== undefined && !!onPageSizeChange);

  const rangeLabel = $derived.by(() => {
    if (totalItems === undefined || pageSize === undefined) return null;
    if (totalItems === 0) return 'Showing 0 of 0';
    const from = (page - 1) * pageSize + 1;
    const to = Math.min(page * pageSize, totalItems);
    return `Showing ${formatNumber(from)}–${formatNumber(to)} of ${formatNumber(totalItems)}`;
  });
</script>

<div class="flex items-center justify-between">
  <p class="text-sm text-muted-foreground">
    {#if rangeLabel}
      {rangeLabel}
    {:else}
      Page {page} of {totalPages}
    {/if}
  </p>
  <div class="flex items-center gap-2">
    {#if hasPageSize}
      <p class="text-sm text-muted-foreground">Rows per page</p>
      <Select.Root
        type="single"
        value={String(pageSize)}
        onValueChange={(v) => onPageSizeChange?.(Number(v))}
        items={PAGE_SIZE_OPTIONS.map((opt) => ({ value: String(opt), label: String(opt) }))}
      >
        <Select.Trigger size="sm" class="w-16" aria-label="Rows per page">
          <SelectPrimitive.Value />
        </Select.Trigger>
        <Select.Content>
          {#each PAGE_SIZE_OPTIONS as opt (opt)}
            <Select.Item value={String(opt)} label={String(opt)}>{opt}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    {/if}
    <Button variant="outline" size="sm" disabled={page <= 1} onclick={() => page--}>
      <ArrowLeft aria-hidden="true" class="mr-1 size-3.5" />
      Previous
    </Button>
    <Button variant="outline" size="sm" disabled={page >= totalPages} onclick={() => page++}>
      Next
      <ArrowRight aria-hidden="true" class="ml-1 size-3.5" />
    </Button>
  </div>
</div>
