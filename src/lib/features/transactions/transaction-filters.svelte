<script lang="ts">
  import { SearchInput } from '$lib/components/shared/search-input';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import FilterBar from '$lib/components/shared/filter-bar.svelte';
  import FilterClearButton from '$lib/components/shared/filter-clear-button.svelte';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import type { TagSummary } from '$lib/tauri';

  /** Select value for "no tag filter" (bits-ui items need a non-empty value). */
  const ANY_TAG = '__any';

  let {
    search = $bindable(''),
    dateFrom = $bindable(''),
    dateTo = $bindable(''),
    tagGuid = $bindable(''),
    tags = [],
    extra = undefined,
    idPrefix = 'txn'
  }: {
    search?: string;
    dateFrom?: string;
    dateTo?: string;
    /** Selected tag; '' = any. */
    tagGuid?: string;
    /** The book's tags; the tag filter only shows when there are some. */
    tags?: TagSummary[];
    /** Optional trailing controls (e.g. a columns toggle), right-aligned. */
    extra?: Snippet;
    /** Prefix for input IDs so multiple filter bars can mount on one page. */
    idPrefix?: string;
  } = $props();

  let hasFilters = $derived(!!(search || dateFrom || dateTo || tagGuid));
  let tagItems = $derived([
    { value: ANY_TAG, label: 'Any tag' },
    ...tags.map((t) => ({ value: t.guid, label: t.name }))
  ]);

  function clearFilters() {
    search = '';
    dateFrom = '';
    dateTo = '';
    tagGuid = '';
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

  {#if tags.length > 0 || tagGuid}
    <div class="flex flex-col gap-1">
      <label for="{idPrefix}-tag" class="text-xs text-muted-foreground">Tag</label>
      <Select.Root
        type="single"
        value={tagGuid || ANY_TAG}
        onValueChange={(v) => (tagGuid = v === ANY_TAG ? '' : v)}
        items={tagItems}
      >
        <Select.Trigger id="{idPrefix}-tag" class="w-40">
          <SelectPrimitive.Value placeholder="Any tag" />
        </Select.Trigger>
        <Select.Content>
          {#each tagItems as item (item.value)}
            <Select.Item value={item.value} label={item.label}>{item.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
  {/if}

  {#if hasFilters}
    <FilterClearButton onclick={clearFilters} />
  {/if}
</FilterBar>
