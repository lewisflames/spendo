<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';

  import { setBudgetAmounts } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { appSettings } from '$lib/stores/settings.svelte';
  import type { BudgetDateRange } from '$lib/tauri';

  let {
    open = $bindable(false),
    accountGuid,
    currency = 'EUR',
    dateRange = null,
    onSave,
    onError
  }: {
    open: boolean;
    accountGuid: string;
    currency?: string;
    dateRange?: BudgetDateRange | null;
    onSave?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  const now = new Date();
  let fromMonth = $state(String(now.getMonth() + 1));
  let fromYear = $state(String(now.getFullYear()));
  let toMonth = $state(String(now.getMonth() + 1));
  let toYear = $state(String(now.getFullYear()));

  let amount = $state(0);

  let loading = $state(false);
  let amountError = $state('');
  let rangeError = $state('');

  const monthIdx = (year: number, month: number) => year * 12 + (month - 1);

  let monthOptions = $derived(
    Array.from({ length: 12 }, (_, i) => ({
      value: String(i + 1),
      label: new Date(2000, i, 1).toLocaleDateString(appSettings.value.locale, { month: 'long' })
    }))
  );

  // Years are limited to the book's transaction range, plus one year ahead so
  // overrides can be planned for the next year.
  let minYear = $derived(
    dateRange?.earliestDate ? Number(dateRange.earliestDate.slice(0, 4)) : null
  );
  let maxYear = $derived(
    dateRange?.latestDate ? Number(dateRange.latestDate.slice(0, 4)) + 1 : null
  );

  let yearOptions = $derived.by(() => {
    if (minYear !== null && maxYear !== null) {
      return Array.from({ length: maxYear - minYear + 1 }, (_, i) => {
        const y = minYear + i;
        return { value: String(y), label: String(y) };
      });
    }
    return Array.from({ length: 21 }, (_, i) => {
      const y = now.getFullYear() - 10 + i;
      return { value: String(y), label: String(y) };
    });
  });

  let monthCount = $derived(
    monthIdx(Number(toYear), Number(toMonth)) - monthIdx(Number(fromYear), Number(fromMonth)) + 1
  );

  function clampYear(year: number): number {
    if (minYear !== null && year < minYear) return minYear;
    if (maxYear !== null && year > maxYear) return maxYear;
    return year;
  }

  // Re-initialize the form whenever the dialog opens (user interaction).
  function syncDefaults() {
    const y = clampYear(new Date().getFullYear());
    const m = new Date().getMonth() + 1;
    fromMonth = String(m);
    fromYear = String(y);
    toMonth = String(m);
    toYear = String(y);
    amount = 0;
    amountError = '';
    rangeError = '';
  }

  async function handleSave() {
    amountError = amount > 0 ? '' : 'Target must be greater than zero';
    rangeError =
      monthIdx(Number(fromYear), Number(fromMonth)) <= monthIdx(Number(toYear), Number(toMonth))
        ? ''
        : 'End month must be on or after the start month';
    if (amount <= 0 || rangeError) return;
    loading = true;
    try {
      await setBudgetAmounts(
        accountGuid,
        Number(fromYear),
        Number(fromMonth),
        Number(toYear),
        Number(toMonth),
        amount
      );
      open = false;
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to save budget overrides'));
    } finally {
      loading = false;
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={(o) => o && syncDefaults()}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Add Budget Overrides</Dialog.Title>
      <Dialog.Description>
        Set the target for a range of months. Existing overrides in that range will be overwritten.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-2">
        <label for="amount" class="text-sm font-medium">Target</label>
        <div class="relative">
          <Input
            id="amount"
            type="number"
            step="0.01"
            min="0"
            bind:value={amount}
            placeholder="0.00"
            class="pr-16"
            aria-invalid={!!amountError}
            aria-describedby={amountError ? 'amount-error' : undefined}
          />
          <span
            class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-muted-foreground uppercase"
            >{currency}</span
          >
        </div>
        {#if amountError}
          <p id="amount-error" role="alert" class="text-xs text-destructive">{amountError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-4">
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">From</span>
          <div class="flex min-w-0 items-center gap-2">
            <Select.Root type="single" bind:value={fromMonth} items={monthOptions}>
              <Select.Trigger id="from-month" class="w-full min-w-0 truncate capitalize">
                <SelectPrimitive.Value />
              </Select.Trigger>
              <Select.Content>
                {#each monthOptions as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            <Select.Root type="single" bind:value={fromYear} items={yearOptions}>
              <Select.Trigger id="from-year" class="min-w-20 justify-center">
                <SelectPrimitive.Value />
              </Select.Trigger>
              <Select.Content>
                {#each yearOptions as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        </div>

        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">To</span>
          <div class="flex min-w-0 items-center gap-2">
            <Select.Root type="single" bind:value={toMonth} items={monthOptions}>
              <Select.Trigger
                id="to-month"
                class="w-full min-w-0 truncate capitalize"
                aria-invalid={!!rangeError}
                aria-describedby={rangeError ? 'range-error' : undefined}
              >
                <SelectPrimitive.Value />
              </Select.Trigger>
              <Select.Content>
                {#each monthOptions as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            <Select.Root type="single" bind:value={toYear} items={yearOptions}>
              <Select.Trigger id="to-year" class="min-w-20 justify-center">
                <SelectPrimitive.Value />
              </Select.Trigger>
              <Select.Content>
                {#each yearOptions as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        </div>
      </div>

      {#if rangeError}
        <p id="range-error" role="alert" class="text-xs text-destructive">{rangeError}</p>
      {/if}

      <p class="text-sm text-muted-foreground">
        {monthCount > 0
          ? `${monthCount} month${monthCount === 1 ? '' : 's'} will be set to the target.`
          : 'Select a valid month range.'}
      </p>
    </div>

    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button onclick={handleSave} disabled={loading}>
        {loading ? 'Saving...' : 'Add Overrides'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
