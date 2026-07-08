<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { PAGE_SIZE_OPTIONS } from '$lib/stores/settings.svelte';
  import { ArrowLeft, ArrowRight } from '@lucide/svelte';

  let {
    page = $bindable(1),
    totalPages = 1,
    pageSize,
    onPageSizeChange
  }: {
    page?: number;
    totalPages?: number;
    pageSize?: number;
    onPageSizeChange?: (size: number) => void;
  } = $props();

  const hasPageSize = $derived(pageSize !== undefined && !!onPageSizeChange);
</script>

<div class="flex items-center justify-between">
  <div class="flex items-center gap-3">
    {#if hasPageSize}
      <Select.Root
        type="single"
        value={String(pageSize)}
        onValueChange={(v) => onPageSizeChange?.(Number(v))}
        items={PAGE_SIZE_OPTIONS.map((opt) => ({ value: String(opt), label: String(opt) }))}
      >
        <Select.Trigger size="sm" class="w-14" aria-label="Rows per page">
          <SelectPrimitive.Value />
        </Select.Trigger>
        <Select.Content>
          {#each PAGE_SIZE_OPTIONS as opt (opt)}
            <Select.Item value={String(opt)} label={String(opt)}>{opt} / page</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    {/if}
    <p class="text-sm text-muted-foreground">
      Page {page} of {totalPages}
    </p>
  </div>
  <div class="flex items-center gap-2">
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
