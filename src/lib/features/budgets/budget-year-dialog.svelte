<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';

  import { setBudgetYearAmounts } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import type { AccountNode, AccountsResult, BudgetDateRange, BulkBudgetResult } from '$lib/tauri';

  let {
    open = $bindable(false),
    accountGuid = null,
    accountsData = null,
    currency = 'EUR',
    dateRange = null,
    initialYear = null,
    onSave,
    onError
  }: {
    open: boolean;
    accountGuid?: string | null;
    accountsData?: AccountsResult | null;
    currency?: string;
    dateRange?: BudgetDateRange | null;
    initialYear?: number | null;
    onSave?: (accountGuid: string, result: BulkBudgetResult) => void;
    onError?: (msg: string) => void;
  } = $props();

  function flattenExpenseAccounts(
    nodes: AccountNode[]
  ): { guid: string; name: string; description: string | null }[] {
    const result: { guid: string; name: string; description: string | null }[] = [];
    function walk(list: AccountNode[]) {
      for (const n of list) {
        if (!n.placeholder && n.accountType === 'EXPENSE') {
          result.push({ guid: n.guid, name: n.name, description: n.description });
        }
        if (n.children.length > 0) walk(n.children);
      }
    }
    walk(nodes);
    return result;
  }

  let expenseAccounts = $derived(
    accountsData?.trees ? flattenExpenseAccounts(accountsData.trees) : []
  );

  const now = new Date();
  let pickedAccountGuid = $state('');
  let fromYear = $state(String(now.getFullYear()));
  let toYear = $state(String(now.getFullYear()));

  let amount = $state(0);

  let loading = $state(false);
  let accountError = $state('');
  let amountError = $state('');
  let rangeError = $state('');

  // Years are limited to the book's transaction range, plus one year ahead so
  // budgets can be planned for the next year.
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

  let yearCount = $derived(Number(toYear) - Number(fromYear) + 1);

  function clampYear(year: number): number {
    if (minYear !== null && year < minYear) return minYear;
    if (maxYear !== null && year > maxYear) return maxYear;
    return year;
  }

  // Re-initialize the form whenever the dialog opens (user interaction).
  function syncDefaults() {
    const y = clampYear(initialYear ?? new Date().getFullYear());
    pickedAccountGuid = '';
    fromYear = String(y);
    toYear = String(y);
    amount = 0;
    accountError = '';
    amountError = '';
    rangeError = '';
  }

  async function handleSave() {
    const target = accountGuid ?? pickedAccountGuid;
    accountError = target ? '' : 'Please select an expense account';
    amountError = amount > 0 ? '' : 'Target must be greater than zero';
    rangeError =
      Number(fromYear) <= Number(toYear) ? '' : 'End year must be on or after the start year';
    if (!target || amount <= 0 || rangeError) return;
    loading = true;
    try {
      const result = await setBudgetYearAmounts(target, Number(fromYear), Number(toYear), amount);
      open = false;
      onSave?.(target, result);
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to save budget'));
    } finally {
      loading = false;
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={(o) => o && syncDefaults()}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Add Budget</Dialog.Title>
      <Dialog.Description>
        Set the annual target for an expense account across a range of years. Only years without a
        budget are created — existing years are kept.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      {#if accountGuid == null}
        <div class="flex flex-col gap-2">
          <label for="account" class="text-sm font-medium">Account</label>
          <Select.Root
            type="single"
            bind:value={pickedAccountGuid}
            items={expenseAccounts.map((a) => ({
              value: a.guid,
              label: a.description || a.name
            }))}
          >
            <Select.Trigger
              id="account"
              class="w-full"
              aria-invalid={!!accountError}
              aria-describedby={accountError ? 'account-error' : undefined}
            >
              <SelectPrimitive.Value placeholder="Select an expense account" />
            </Select.Trigger>
            <Select.Content>
              {#each expenseAccounts as a (a.guid)}
                <Select.Item value={a.guid} label={a.description || a.name}
                  >{a.description || a.name}</Select.Item
                >
              {/each}
            </Select.Content>
          </Select.Root>
          {#if accountError}
            <p id="account-error" role="alert" class="text-xs text-destructive">{accountError}</p>
          {/if}
        </div>
      {/if}

      <div class="flex flex-col gap-2">
        <label for="amount" class="text-sm font-medium">Annual target</label>
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
          <span class="text-sm font-medium">From year</span>
          <Select.Root type="single" bind:value={fromYear} items={yearOptions}>
            <Select.Trigger
              id="from-year"
              class="min-w-20 justify-center"
              aria-invalid={!!rangeError}
              aria-describedby={rangeError ? 'range-error' : undefined}
            >
              <SelectPrimitive.Value />
            </Select.Trigger>
            <Select.Content>
              {#each yearOptions as o (o.value)}
                <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>

        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">To year</span>
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

      {#if rangeError}
        <p id="range-error" role="alert" class="text-xs text-destructive">{rangeError}</p>
      {/if}

      <p class="text-sm text-muted-foreground">
        {yearCount > 0
          ? `${yearCount} year${yearCount === 1 ? '' : 's'} selected. Existing budgets are kept.`
          : 'Select a valid year range.'}
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
        {loading ? 'Saving...' : 'Add Budget'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
