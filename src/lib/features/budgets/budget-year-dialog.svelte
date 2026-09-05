<script lang="ts">
  import { untrack } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';

  import { getBudgetYears, setBudgetYearAmount } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import type { AccountNode, AccountsResult, BudgetDateRange } from '$lib/tauri';

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
    onSave?: (accountGuid: string, year: number) => void;
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
  let year = $state(String(now.getFullYear()));

  let amount = $state(0);

  let loading = $state(false);
  let accountError = $state('');
  let amountError = $state('');
  let yearError = $state('');

  // Years already budgeted for the target account. Null = unknown
  // (no account selected yet, or not fetched).
  let existingYears = $state<number[] | null>(null);
  let yearsLoading = $state(false);

  let target = $derived(accountGuid ?? (pickedAccountGuid || null));

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

  let takenSet = $derived(new Set(existingYears ?? []));

  let freeYears = $derived(yearOptions.filter((o) => !takenSet.has(Number(o.value))));

  function clampYear(y: number): number {
    if (minYear !== null && y < minYear) return minYear;
    if (maxYear !== null && y > maxYear) return maxYear;
    return y;
  }

  // Re-initialize the form whenever the dialog opens (user interaction).
  // This must be an $effect: bits-ui only calls onOpenChange for its own
  // internal open/close (trigger, Escape), not when `open` is set
  // programmatically via bind:open — which is the only way this dialog is
  // opened — so syncing there never runs.
  $effect.pre(() => {
    if (open) untrack(() => syncDefaults());
  });

  function syncDefaults() {
    const y = clampYear(initialYear ?? new Date().getFullYear());
    pickedAccountGuid = '';
    year = String(y);
    amount = 0;
    accountError = '';
    amountError = '';
    yearError = '';
    existingYears = null;
  }

  // Load taken years whenever the dialog is open and the target account is
  // known (prop account or picked account on the list page).
  $effect(() => {
    if (!open || !target) {
      return;
    }
    const guid = target;
    let cancelled = false;
    yearsLoading = true;
    getBudgetYears(guid)
      .then((years) => {
        if (cancelled) return;
        existingYears = years;
        // Move off a taken year automatically so Save never starts invalid.
        if (years.includes(Number(year))) {
          const fallback = yearOptions.find((o) => !years.includes(Number(o.value)));
          if (fallback) year = fallback.value;
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        existingYears = null;
        onError?.(errMsg(e, 'Failed to load existing budgets'));
      })
      .finally(() => {
        if (!cancelled) yearsLoading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  async function handleSave() {
    const y = Number(year);
    accountError = target ? '' : 'Please select an expense account';
    amountError = amount > 0 ? '' : 'Target must be greater than zero';
    yearError =
      existingYears !== null && existingYears.includes(y)
        ? 'This account already has a budget for the selected year'
        : '';
    if (!target || amount <= 0 || yearError) return;
    loading = true;
    try {
      await setBudgetYearAmount(target, y, amount);
      open = false;
      onSave?.(target, y);
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to save budget'));
    } finally {
      loading = false;
    }
  }

  let noFreeYears = $derived(target !== null && existingYears !== null && freeYears.length === 0);
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Add Budget</Dialog.Title>
      <Dialog.Description>
        Set the annual target for an expense account for a single year. Years that already have a
        budget are disabled.
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

      <div class="flex flex-col gap-2">
        <span class="text-sm font-medium">Year</span>
        <Select.Root type="single" bind:value={year} items={yearOptions}>
          <Select.Trigger
            id="year"
            class="min-w-20 justify-center"
            aria-invalid={!!yearError}
            aria-describedby={yearError ? 'year-error' : undefined}
          >
            <SelectPrimitive.Value />
          </Select.Trigger>
          <Select.Content>
            {#each yearOptions as o (o.value)}
              {@const taken = takenSet.has(Number(o.value))}
              <Select.Item value={o.value} label={o.label} disabled={taken}>
                {o.label}{taken ? ' — has budget' : ''}
              </Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>

      {#if yearError}
        <p id="year-error" role="alert" class="text-xs text-destructive">{yearError}</p>
      {/if}

      <p class="text-sm text-muted-foreground">
        {#if !target}
          Select an account to see available years.
        {:else if yearsLoading || existingYears === null}
          Loading existing budgets…
        {:else if noFreeYears}
          This account already has a budget for every available year.
        {:else}
          Years with a budget are disabled and cannot be selected.
        {/if}
      </p>
    </div>

    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button
        onclick={handleSave}
        disabled={loading ||
          yearsLoading ||
          (target !== null && existingYears === null) ||
          noFreeYears}
      >
        {loading ? 'Saving...' : 'Add Budget'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
