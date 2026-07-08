<script lang="ts">
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { SearchInput } from '$lib/components/shared/search-input';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import { Button } from '$lib/components/ui/button';
  import { X } from '@lucide/svelte';
  import type { AccountInfo } from '$lib/tauri';

  let {
    search = $bindable(''),
    dateFrom = $bindable(''),
    dateTo = $bindable(''),
    accountName = $bindable(''),
    accounts = []
  }: {
    search?: string;
    dateFrom?: string;
    dateTo?: string;
    accountName?: string;
    accounts?: AccountInfo[];
  } = $props();

  let hasFilters = $derived(!!(search || dateFrom || dateTo || accountName));

  function clearFilters() {
    search = '';
    dateFrom = '';
    dateTo = '';
    accountName = '';
  }
</script>

<div class="flex flex-wrap items-end gap-2">
  <SearchInput
    id="txn-search"
    label="Search description, notes, memo, number"
    placeholder="Search description, notes, memo, number..."
    bind:value={search}
    class="grow basis-48"
  />

  <div class="flex flex-col gap-1">
    <label for="txn-from" class="text-xs text-muted-foreground">From</label>
    <DatePicker id="txn-from" bind:value={dateFrom} placeholder="Start date" />
  </div>

  <div class="flex flex-col gap-1">
    <label for="txn-to" class="text-xs text-muted-foreground">To</label>
    <DatePicker id="txn-to" bind:value={dateTo} placeholder="End date" />
  </div>

  <div class="flex flex-col gap-1">
    <label for="txn-account" class="text-xs text-muted-foreground">Account</label>
    <Select.Root type="single" bind:value={accountName}>
      <Select.Trigger id="txn-account" class="min-w-36">
        <SelectPrimitive.Value placeholder="All accounts" />
      </Select.Trigger>
      <Select.Content>
        <Select.Item value="">All accounts</Select.Item>
        {#each accounts as acc (acc.name)}
          <Select.Item value={acc.name}>{acc.name}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </div>
  {#if hasFilters}
    <Button variant="ghost" onclick={clearFilters}>
      <X aria-hidden="true" class="mr-1 size-3" />
      Clear
    </Button>
  {/if}
</div>
