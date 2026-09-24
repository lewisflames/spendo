<script lang="ts">
  import { SearchInput } from '$lib/components/shared/search-input';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
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

<FilterBar {extra}>
  <SearchInput
    id="{idPrefix}-search"
    label="Search description, notes, memo, number"
    placeholder="Search anything: description, notes, number…"
    bind:value={search}
    class="grow basis-48"
  />

  <div class="flex flex-col gap-1">
    <label for="{idPrefix}-from" class="text-xs text-muted-foreground">From</label>
    <DatePicker id="{idPrefix}-from" bind:value={dateFrom} placeholder="Start date" clearable />
  </div>

  <div class="flex flex-col gap-1">
    <label for="{idPrefix}-to" class="text-xs text-muted-foreground">To</label>
    <DatePicker id="{idPrefix}-to" bind:value={dateTo} placeholder="End date" clearable />
  </div>

  {#if hasFilters}
    <FilterClearButton onclick={clearFilters} />
  {/if}
</FilterBar>
