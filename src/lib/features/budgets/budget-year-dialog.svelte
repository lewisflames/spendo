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
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
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
  let showErrors = $state(false);

  // Years already budgeted for the target account. Null = unknown
  // (no account selected yet, or not fetched).
  let existingYears = $state<number[] | null>(null);
  let yearsLoading = $state(false);

  let target = $derived(accountGuid ?? (pickedAccountGuid || null));

  // Years are limited to the book’s transaction range, plus one year ahead so
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
    showErrors = false;
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
        const msg = errMsg(e, 'We couldn’t load your budgets. Please try again');
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

  let accountError = $derived(target ? '' : 'Choose an expense account to continue');
  let amountError = $derived(amount > 0 ? '' : 'Enter an amount above zero');
  let yearError = $derived(
    existingYears !== null && existingYears.includes(Number(by.yearSel))
      ? 'There’s already a budget for this year. Pick another one'
      : ''
  );

  let noFreeYears = $derived(target !== null && existingYears !== null && freeYears.length === 0);
  let canSave = $derived(
    !loading &&
      !yearsLoading &&
      target !== null &&
      existingYears !== null &&
      !noFreeYears &&
      amount > 0 &&
      !yearError
  );
  let saveDisabled = $derived(!canSave);

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
        const msg = errMsg(e, 'We couldn’t load your budgets. Please try again');
        fetchError = msg;
        onError?.(msg);
      })
      .finally(() => {
        yearsLoading = false;
      });
  }

  async function handleSave() {
    const y = Number(by.yearSel);
    if (loading) return;
    if (!canSave || !target) {
      showErrors = true;
      return;
    }
    loading = true;
    formError = '';
    try {
      await setBudgetYearAmount(target, y, amount);
      open = false;
      onSave?.(target, y);
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t save this budget. Please try again');
    } finally {
      loading = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>New budget</Dialog.Title>
      <Dialog.Description>
        How much do you plan to spend from this account this year? Years you’ve already budgeted are
        greyed out.
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
              label: a.name
            }))}
          >
            <Select.Trigger
              id="account"
              class="w-full"
              aria-invalid={showErrors && !!accountError}
              aria-describedby={showErrors && accountError ? 'account-error' : undefined}
            >
              <SelectPrimitive.Value placeholder="Select an expense account" />
            </Select.Trigger>
            <Select.Content>
              {#each expenseAccounts as a (a.guid)}
                <Select.Item value={a.guid} label={a.name}>
                  <AccountLabel name={a.name} description={a.description} code={a.code} />
                </Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          {#if showErrors && accountError}
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
            aria-invalid={showErrors && !!amountError}
            aria-describedby={showErrors && amountError ? 'amount-error' : undefined}
          />
          <span
            class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-muted-foreground uppercase"
            >{currency}</span
          >
        </div>
        {#if showErrors && amountError}
          <p id="amount-error" role="alert" class="text-xs text-destructive">{amountError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="year" class="text-sm font-medium">Year</label>
        <Select.Root type="single" bind:value={by.yearSel} items={yearOptions}>
          <Select.Trigger
            id="year"
            class="min-w-20 justify-center"
            aria-invalid={showErrors && !!yearError}
            aria-describedby={showErrors && yearError ? 'year-error' : undefined}
          >
            <SelectPrimitive.Value />
          </Select.Trigger>
          <Select.Content>
            {#each yearOptions as o (o.value)}
              {@const taken = takenSet.has(Number(o.value))}
              <Select.Item value={o.value} label={o.label} disabled={taken}>
                {o.label}{taken ? ' (has budget)' : ''}
              </Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>

      {#if showErrors && yearError}
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
          Pick an account to see which years are still free.
        {:else if yearsLoading}
          Checking which years are taken…
        {:else if existingYears === null}
          We couldn’t load your budgets. Retry to see which years are taken.
        {:else if noFreeYears}
          Every available year already has a budget for this account.
        {:else}
          Years with a budget are greyed out.
        {/if}
      </p>
    </div>

    <SaveFooter
      mode="create"
      saving={loading}
      deleting={false}
      confirmDelete={false}
      saveLabel="Create budget"
      {saveDisabled}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
