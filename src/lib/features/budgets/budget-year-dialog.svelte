<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';

  import { fetchBudgetYears, setBudgetYearAmount } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import type { AccountsResult, BudgetDateRange } from '$lib/tauri';
  import { flattenExpenseAccounts } from './budget-utils';
  import { useBudgetYear } from '$lib/hooks/use-budget-year.svelte';

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

  let expenseAccounts = $derived(
    accountsData?.trees ? flattenExpenseAccounts(accountsData.trees) : []
  );

  const now = new Date();
  let pickedAccountGuid = $state('');

  let amount = $state(0);

  let loading = $state(false);
  let formError = $state('');
  let fetchError = $state('');

  // Years already budgeted for the target account. Null = unknown
  // (no account selected yet, or not fetched).
  let existingYears = $state<number[] | null>(null);
  let yearsLoading = $state(false);

  let target = $derived(accountGuid ?? (pickedAccountGuid || null));

  // Years are limited to the book's transaction range, plus one year ahead so
  // budgets can be planned for the next year.
  const by = useBudgetYear(() => dateRange, {
    plusOne: true,
    fallbackCenter: now.getFullYear(),
    fallbackRadius: 10
  });
  let yearOptions = $derived(by.yearOptions);

  let takenSet = $derived(new Set(existingYears ?? []));

  let freeYears = $derived(yearOptions.filter((o) => !takenSet.has(Number(o.value))));

  // Re-initialize the form whenever the dialog opens (user interaction).
  useDialogReset(() => open, syncDefaults);

  function syncDefaults() {
    const y = by.clampYear(initialYear ?? new Date().getFullYear());
    pickedAccountGuid = '';
    by.yearSel = String(y);
    amount = 0;
    formError = '';
    fetchError = '';
    existingYears = null;
    yearsLoading = false;
    loading = false;
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
    fetchError = '';
    fetchBudgetYears(guid)
      .then((years) => {
        if (cancelled) return;
        existingYears = years;
        // Move off a taken year automatically so Save never starts invalid.
        if (years.includes(Number(by.yearSel))) {
          const fallback = yearOptions.find((o) => !years.includes(Number(o.value)));
          if (fallback) by.yearSel = fallback.value;
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        existingYears = null;
        const msg = errMsg(e, 'Failed to load existing budgets');
        fetchError = msg;
        onError?.(msg);
      })
      .finally(() => {
        if (!cancelled) yearsLoading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  let accountError = $derived(target ? '' : 'Please select an expense account');
  let amountError = $derived(amount > 0 ? '' : 'Target must be greater than zero');
  let yearError = $derived(
    existingYears !== null && existingYears.includes(Number(by.yearSel))
      ? 'This account already has a budget for the selected year'
      : ''
  );

  let noFreeYears = $derived(target !== null && existingYears !== null && freeYears.length === 0);
  let saveDisabled = $derived(
    loading ||
      yearsLoading ||
      (target !== null && existingYears === null) ||
      noFreeYears ||
      !target ||
      amount <= 0 ||
      !!yearError
  );

  function retryFetchYears() {
    existingYears = null;
    fetchError = '';
    yearsLoading = false;
    // Trigger refetch by touching target via reload: simplest is to re-run
    // the effect by bumping yearSel twice (no-op) — instead, fetch directly.
    const guid = target;
    if (!guid) return;
    yearsLoading = true;
    fetchBudgetYears(guid)
      .then((years) => {
        existingYears = years;
        if (years.includes(Number(by.yearSel))) {
          const fallback = yearOptions.find((o) => !years.includes(Number(o.value)));
          if (fallback) by.yearSel = fallback.value;
        }
      })
      .catch((e: unknown) => {
        existingYears = null;
        const msg = errMsg(e, 'Failed to load existing budgets');
        fetchError = msg;
        onError?.(msg);
      })
      .finally(() => {
        yearsLoading = false;
      });
  }

  async function handleSave() {
    const y = Number(by.yearSel);
    if (!target || amount <= 0 || yearError) return;
    loading = true;
    formError = '';
    try {
      await setBudgetYearAmount(target, y, amount);
      open = false;
      onSave?.(target, y);
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save budget');
      formError = msg;
      onError?.(msg);
    } finally {
      loading = false;
    }
  }
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
        <label id="budget-year-label" for="year" class="text-sm font-medium">Year</label>
        <Select.Root type="single" bind:value={by.yearSel} items={yearOptions}>
          <Select.Trigger
            id="year"
            class="min-w-20 justify-center"
            aria-invalid={!!yearError}
            aria-describedby={yearError ? 'year-error' : undefined}
            aria-labelledby="budget-year-label"
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
      {#if fetchError}
        <p role="alert" class="text-sm text-destructive">
          {fetchError}
          <button type="button" class="ml-2 underline underline-offset-2" onclick={retryFetchYears}>
            Retry
          </button>
        </p>
      {/if}
      {#if formError}
        <p role="alert" class="text-sm text-destructive">{formError}</p>
      {/if}

      <p class="text-sm text-muted-foreground">
        {#if !target}
          Select an account to see available years.
        {:else if yearsLoading}
          Loading existing budgets…
        {:else if existingYears === null}
          Could not load existing budgets. Retry to see which years are taken.
        {:else if noFreeYears}
          This account already has a budget for every available year.
        {:else}
          Years with a budget are disabled and cannot be selected.
        {/if}
      </p>
    </div>

    <SaveFooter
      mode="create"
      saving={loading}
      deleting={false}
      confirmDelete={false}
      saveLabel="Add budget"
      {saveDisabled}
      onDelete={() => {}}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
