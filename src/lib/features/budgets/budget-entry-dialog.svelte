<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { Input } from '$lib/components/ui/input';

  import { setBudgetYearAmount, deleteBudgetYear } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';

  let {
    open = $bindable(false),
    accountGuid,
    year,
    amount: initialAmount,
    exists = true,
    currency = 'EUR',
    accountName,
    onSave,
    onDelete
  }: {
    open: boolean;
    accountGuid: string;
    year: number;
    amount: number;
    exists?: boolean;
    currency?: string;
    accountName?: string;
    onSave?: () => void;
    onDelete?: () => void;
  } = $props();

  let amount = $state(0);

  let loading = $state(false);
  let deleting = $state(false);
  let confirmDelete = $state(false);
  let formError = $state('');
  let showErrors = $state(false);

  let amountError = $derived(amount > 0 ? '' : 'Enter an amount above zero');
  let canSave = $derived(!loading && amount > 0);

  // Re-initialize the form whenever the dialog opens (user interaction), so
  // it always reflects the target year.
  useDialogReset(() => open, syncFromEntry);

  function syncFromEntry() {
    amount = initialAmount;
    formError = '';
    showErrors = false;
    loading = false;
    deleting = false;
    confirmDelete = false;
  }

  async function handleSave() {
    if (loading) return;
    if (!canSave) {
      showErrors = true;
      return;
    }
    loading = true;
    formError = '';
    try {
      await setBudgetYearAmount(accountGuid, year, amount);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t save this budget');
    } finally {
      loading = false;
    }
  }

  async function handleDelete() {
    if (!exists || deleting) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    deleting = true;
    formError = '';
    try {
      await deleteBudgetYear(accountGuid, year);
      open = false;
      onDelete?.();
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t delete this budget');
    } finally {
      deleting = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{exists ? 'Edit budget' : 'Set budget'}</Dialog.Title>
      <Dialog.Description>
        {exists
          ? 'Tweak what you plan to spend this year.'
          : 'Set what you plan to spend this year.'}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-2">
        <span id="budget-year-label" class="text-sm font-medium">Year</span>
        <p class="text-sm" aria-labelledby="budget-year-label">{year}</p>
      </div>

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
            autofocus
          />
          <span
            class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-muted-foreground uppercase"
            >{currency}</span
          >
        </div>
        {#if showErrors && amountError}
          <p id="amount-error" role="alert" class="text-xs text-destructive">{amountError}</p>
        {/if}
        {#if formError}
          <p role="alert" class="text-sm text-destructive">{formError}</p>
        {/if}
      </div>

      {#if exists && confirmDelete}
        <p
          role="alert"
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          Delete the {year} budget for <strong>{accountName ?? 'this account'}</strong>? Other years
          stay put. You can’t undo this.
        </p>
      {/if}
    </div>

    <SaveFooter
      mode={exists ? 'edit' : 'create'}
      saving={loading}
      {deleting}
      {confirmDelete}
      saveLabel={exists ? 'Save changes' : 'Create budget'}
      saveDisabled={!canSave}
      onDelete={handleDelete}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
