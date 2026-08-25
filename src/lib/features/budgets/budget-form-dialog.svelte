<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';

  import { setBudget, getEarliestTransactionDate, getGlobalEarliestDate } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { formatCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import type { Budget, AccountsResult } from '$lib/tauri';

  let {
    open = $bindable(false),
    budget = null,
    accountsData = null,
    viewYear = new Date().getFullYear(),
    currency = 'EUR',
    onSave,
    onError
  }: {
    open: boolean;
    budget?: Budget | null;
    accountsData?: AccountsResult | null;
    viewYear?: number;
    currency?: string;
    onSave?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let addAccountGuid = $state('');
  let addAmount = $state(0);
  let addPeriodType = $state('MONTHLY');
  let rolloverStartOption = $state<string>('none');
  let rolloverStartDate = $state('');
  let addNotes = $state('');
  let earliestTxDate = $state<string | null>(null);
  let loadingEarliestTx = $state(false);
  let globalEarliestDate = $state<string | null>(null);
  let addRollover = $derived(rolloverStartOption !== 'none');

  let addLoading = $state(false);
  let addAccountError = $state('');
  let addAmountError = $state('');

  interface AccountTreeNode {
    guid: string;
    name: string;
    description: string | null;
    accountType: string;
    placeholder: boolean;
    children: AccountTreeNode[];
  }

  function flattenExpenseAccounts(
    nodes: AccountTreeNode[]
  ): { guid: string; name: string; description: string | null }[] {
    const result: { guid: string; name: string; description: string | null }[] = [];
    function walk(list: AccountTreeNode[]) {
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

  let addAccounts = $derived(accountsData?.trees ? flattenExpenseAccounts(accountsData.trees) : []);

  // Parse the 'YYYY-MM-DD' prefix of a stored date as a local calendar date
  // so displayed months match the stored value exactly (no TZ shifting).
  // Matches the backend, which also slices the raw UTC date prefix.
  let creationMonth = $derived.by(() => {
    if (!budget?.createdAt) return '';
    return parseLocalDate(budget.createdAt).toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'long'
    });
  });

  let currentYearLabel = $derived(
    new Date(viewYear, 0).toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'long'
    })
  );

  let prevYearLabel = $derived(
    new Date(viewYear - 1, 0).toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'long'
    })
  );

  let firstTxMonthLabel = $derived.by(() => {
    if (loadingEarliestTx) return 'Loading…';
    if (!earliestTxDate) return '';
    return parseLocalDate(earliestTxDate).toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'long'
    });
  });

  let globalEarliestLabel = $derived.by(() => {
    if (!globalEarliestDate) return '';
    return parseLocalDate(globalEarliestDate).toLocaleDateString(appSettings.value.locale, {
      year: 'numeric',
      month: 'long'
    });
  });

  let rolloverOptions = $derived([
    { value: 'none', label: 'Do not carry over', disabled: false },
    {
      value: 'creation',
      label: budget?.createdAt
        ? `Budget creation month (${creationMonth})`
        : 'Budget creation month',
      disabled: false
    },
    {
      value: 'current_year',
      label: `Start of current year (${currentYearLabel})`,
      disabled: false
    },
    {
      value: 'prev_year',
      label: `Start of previous year (${prevYearLabel})`,
      disabled: false
    },
    {
      value: 'first_tx',
      label: `First transaction in this account${firstTxMonthLabel ? ` (${firstTxMonthLabel})` : ''}`,
      disabled: false
    },
    {
      value: 'all_data',
      label: `Beginning of all data${globalEarliestLabel ? ` (${globalEarliestLabel})` : ''}`,
      disabled: !globalEarliestDate
    },
    {
      value: 'custom',
      label: 'Custom month',
      disabled: false
    }
  ]);

  function resetForm() {
    addAccountGuid = '';
    addAmount = 0;
    addPeriodType = 'MONTHLY';
    rolloverStartOption = 'none';
    rolloverStartDate = '';
    addNotes = '';
    earliestTxDate = null;
    addAccountError = '';
    addAmountError = '';
  }

  // Initialize form fields when the target budget changes. Deliberately does
  // NOT read earliestTxDate/globalEarliestDate: async arrivals must never
  // rewrite fields the user is editing.
  $effect(() => {
    const b = budget;
    if (b) {
      addAccountGuid = b.accountGuid;
      addAmount = b.amount;
      addPeriodType = b.periodType;
      addNotes = b.notes ?? '';
      earliestTxDate = null;

      if (!b.rollover || !b.rolloverStart) {
        rolloverStartOption = 'none';
        rolloverStartDate = '';
      } else {
        const startYm = b.rolloverStart.slice(0, 7);
        const creationYm = b.createdAt.slice(0, 7);
        const matched =
          startYm === creationYm
            ? 'creation'
            : startYm === `${viewYear}-01`
              ? 'current_year'
              : startYm === `${viewYear - 1}-01`
                ? 'prev_year'
                : null;
        if (matched) {
          rolloverStartOption = matched;
          rolloverStartDate = '';
        } else {
          rolloverStartOption = 'custom';
          rolloverStartDate = startYm;
        }
      }
    } else {
      resetForm();
    }
  });

  // When editing a budget whose carryover start matched no named option at
  // init (async dates not yet loaded), upgrade the unresolved 'custom' to the
  // matching named option. Called only from the async arrival points below —
  // NOT reactively — so a user deliberately picking 'Custom' afterwards is
  // never snapped back.
  function maybeUpgradeRolloverOption() {
    const b = budget;
    if (!b?.rollover || !b.rolloverStart || rolloverStartOption !== 'custom') return;
    const startYm = b.rolloverStart.slice(0, 7);
    if (globalEarliestDate && startYm === globalEarliestDate.slice(0, 7)) {
      rolloverStartOption = 'all_data';
      rolloverStartDate = '';
    } else if (earliestTxDate && startYm === earliestTxDate.slice(0, 7)) {
      rolloverStartOption = 'first_tx';
      rolloverStartDate = '';
    }
  }

  $effect(() => {
    const guid = addAccountGuid;
    if (!guid) {
      earliestTxDate = null;
      return;
    }
    loadingEarliestTx = true;
    getEarliestTransactionDate(guid)
      .then((d) => {
        if (guid === addAccountGuid) {
          earliestTxDate = d;
          maybeUpgradeRolloverOption();
        }
      })
      .catch(() => {
        if (guid === addAccountGuid) earliestTxDate = null;
      })
      .finally(() => {
        if (guid === addAccountGuid) loadingEarliestTx = false;
      });
  });

  $effect(() => {
    const isOpen = open;
    if (!isOpen) return;
    globalEarliestDate = null;
    getGlobalEarliestDate()
      .then((d) => {
        // Drop stale results if the dialog closed or reopened mid-flight.
        if (open !== isOpen) return;
        globalEarliestDate = d;
        maybeUpgradeRolloverOption();
      })
      .catch(() => {});
  });

  function resolveRolloverStart(): string | undefined {
    if (rolloverStartOption === 'none') return undefined;
    switch (rolloverStartOption) {
      case 'creation':
        return undefined;
      case 'current_year':
        return `${viewYear}-01-01`;
      case 'prev_year':
        return `${viewYear - 1}-01-01`;
      case 'first_tx': {
        if (earliestTxDate) return earliestTxDate.slice(0, 7) + '-01';
        return undefined;
      }
      case 'all_data':
        if (globalEarliestDate) return globalEarliestDate.slice(0, 7) + '-01';
        return undefined;
      case 'custom':
        if (rolloverStartDate) return rolloverStartDate + '-01';
        return undefined;
    }
  }

  async function handleSave() {
    addAccountError = addAccountGuid ? '' : 'Please select an expense account';
    addAmountError = addAmount > 0 ? '' : 'Target must be greater than zero';
    if (!addAccountGuid || addAmount <= 0) return;
    addLoading = true;
    try {
      await setBudget(
        addAccountGuid,
        addAmount,
        addPeriodType,
        addRollover,
        resolveRolloverStart(),
        addNotes.trim() || undefined
      );
      open = false;
      resetForm();
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, budget ? 'Failed to update budget' : 'Failed to add budget'));
    } finally {
      addLoading = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{budget ? 'Edit Budget' : 'Add Budget'}</Dialog.Title>
      <Dialog.Description>
        {budget
          ? 'Update the spending target for this account.'
          : 'Set a spending target for an expense account.'}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-2">
        <label for="account" class="text-sm font-medium">Account</label>
        <Select.Root
          type="single"
          bind:value={addAccountGuid}
          disabled={!!budget}
          items={addAccounts.map((a) => ({ value: a.guid, label: a.description || a.name }))}
        >
          <Select.Trigger
            id="account"
            class="w-full"
            aria-invalid={!!addAccountError}
            aria-describedby={addAccountError ? 'account-error' : undefined}
          >
            <SelectPrimitive.Value placeholder="Select an expense account" />
          </Select.Trigger>
          <Select.Content>
            {#each addAccounts as a (a.guid)}
              <Select.Item value={a.guid} label={a.description || a.name}
                >{a.description || a.name}</Select.Item
              >
            {/each}
          </Select.Content>
        </Select.Root>
        {#if addAccountError}
          <p id="account-error" role="alert" class="text-xs text-destructive">{addAccountError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="amount" class="text-sm font-medium">Target</label>
        <div class="relative">
          <Input
            id="amount"
            type="number"
            step="0.01"
            min="0"
            bind:value={addAmount}
            placeholder="0.00"
            class="pr-16"
            aria-invalid={!!addAmountError}
            aria-describedby={addAmountError ? 'amount-error' : undefined}
          />
          <span
            class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-muted-foreground uppercase"
            >{currency}</span
          >
        </div>
        {#if addAmountError}
          <p id="amount-error" role="alert" class="text-xs text-destructive">{addAmountError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="period-type" class="text-sm font-medium">Period</label>
        <Select.Root
          type="single"
          bind:value={addPeriodType}
          items={[
            { value: 'MONTHLY', label: 'Monthly' },
            { value: 'YEARLY', label: 'Yearly' }
          ]}
        >
          <Select.Trigger class="w-full" id="period-type">
            <SelectPrimitive.Value />
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="MONTHLY" label="Monthly">Monthly</Select.Item>
            <Select.Item value="YEARLY" label="Yearly">Yearly</Select.Item>
          </Select.Content>
        </Select.Root>
        {#if addPeriodType === 'YEARLY'}
          <p class="text-xs text-muted-foreground">
            The target will be divided by 12 for a monthly comparison ({formatCurrency(
              addAmount / 12,
              currency
            )}/mo).
          </p>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="carryover" class="text-sm font-medium">Carryover</label>
        <p class="text-xs text-muted-foreground">
          Surplus rolls forward; deficits reduce next month&rsquo;s effective target.
        </p>

        <Select.Root type="single" bind:value={rolloverStartOption} items={rolloverOptions}>
          <Select.Trigger class="w-full" id="carryover">
            <SelectPrimitive.Value placeholder="Select a carryover option" />
          </Select.Trigger>
          <Select.Content>
            {#each rolloverOptions as opt (opt.value)}
              <Select.Item value={opt.value} label={opt.label} disabled={opt.disabled}>
                {opt.label}
              </Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>

        {#if rolloverStartOption === 'custom'}
          <div class="flex items-center gap-2">
            <Input type="month" bind:value={rolloverStartDate} class="w-fit" />
          </div>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="notes" class="text-sm font-medium">Notes</label>
        <Textarea
          id="notes"
          bind:value={addNotes}
          placeholder="Optional notes about this budget"
          class="min-h-20"
        />
      </div>
    </div>

    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button onclick={handleSave} disabled={addLoading}>
        {addLoading ? 'Saving...' : budget ? 'Save Changes' : 'Save Budget'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
