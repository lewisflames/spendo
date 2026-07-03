<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { DatePicker } from '$lib/components/ui/date-picker';
  import { Button } from '$lib/components/ui/button';
  import { Download, Search, X } from '@lucide/svelte';
  import type { AccountInfo } from '$lib/server/transactions';

  let {
    search = $bindable(''),
    dateFrom = $bindable(''),
    dateTo = $bindable(''),
    accountName = $bindable(''),
    accounts = [],
    showExport = false,
    onExport
  }: {
    search?: string;
    dateFrom?: string;
    dateTo?: string;
    accountName?: string;
    accounts?: AccountInfo[];
    showExport?: boolean;
    onExport?: () => Promise<void>;
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
  <div class="relative grow basis-48">
    <Search
      aria-hidden="true"
      class="text-muted-foreground pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2"
    />
    <Input class="pl-8" placeholder="Search description..." bind:value={search} />
  </div>

  <div class="flex flex-col gap-1">
    <span class="text-xs text-muted-foreground">From</span>
    <DatePicker bind:value={dateFrom} placeholder="Start date" />
  </div>

  <div class="flex flex-col gap-1">
    <span class="text-xs text-muted-foreground">To</span>
    <DatePicker bind:value={dateTo} placeholder="End date" />
  </div>

  <div class="flex flex-col gap-1">
    <span class="text-xs text-muted-foreground">Account</span>
    <Select.Root type="single" bind:value={accountName}>
      <Select.Trigger class="min-w-36">
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
    <Button variant="destructive" onclick={clearFilters}>
      <X aria-hidden="true" class="mr-1 size-3" />
      Clear
    </Button>
  {/if}

  {#if showExport}
    <Button variant="outline" class="ml-auto" onclick={onExport}>
      <Download aria-hidden="true" class="mr-1 size-3.5" />
      CSV
    </Button>
  {/if}
</div>
