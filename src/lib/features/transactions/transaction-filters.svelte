<script lang="ts">
  import { SearchInput } from '$lib/components/shared/search-input';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import { Button } from '$lib/components/ui/button';
  import { X } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  let {
    search = $bindable(''),
    dateFrom = $bindable(''),
    dateTo = $bindable(''),
    extra = undefined,
    idPrefix = 'txn'
  }: {
    search?: string;
    dateFrom?: string;
    dateTo?: string;
    /** Optional trailing controls (e.g. a columns toggle), right-aligned. */
    extra?: Snippet;
    /** Prefix for input IDs so multiple filter bars can mount on one page. */
    idPrefix?: string;
  } = $props();

  let hasFilters = $derived(!!(search || dateFrom || dateTo));

  function clearFilters() {
    search = '';
    dateFrom = '';
    dateTo = '';
  }
</script>

<div class="flex flex-wrap items-end gap-2">
  <SearchInput
    id="{idPrefix}-search"
    label="Search description, notes, memo, number"
    placeholder="Search description, notes, memo, number…"
    bind:value={search}
    class="grow basis-48"
  />

  <div class="flex flex-col gap-1">
    <label for="{idPrefix}-from" class="text-xs text-muted-foreground">From</label>
    <DatePicker id="{idPrefix}-from" bind:value={dateFrom} placeholder="Start date" />
  </div>

  <div class="flex flex-col gap-1">
    <label for="{idPrefix}-to" class="text-xs text-muted-foreground">To</label>
    <DatePicker id="{idPrefix}-to" bind:value={dateTo} placeholder="End date" />
  </div>

  {#if hasFilters}
    <Button variant="ghost" onclick={clearFilters}>
      <X aria-hidden="true" class="mr-1 size-3.5" />
      Clear filters
    </Button>
  {/if}
  {#if extra}
    <div class="ml-auto">{@render extra()}</div>
  {/if}
</div>
