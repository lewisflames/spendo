<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { Input } from '$lib/components/ui/input';

  import { setBudgetYearAmount } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';

  let {
    open = $bindable(false),
    accountGuid,
    year,
    amount: initialAmount,
    exists = true,
    currency = 'EUR',
    onSave,
    onError
  }: {
    open: boolean;
    accountGuid: string;
    year: number;
    amount: number;
    exists?: boolean;
    currency?: string;
    onSave?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let amount = $state(0);

  let loading = $state(false);
  let formError = $state('');

  let amountError = $derived(amount > 0 ? '' : 'Target must be greater than zero');
  let saveDisabled = $derived(loading || amount <= 0);

  // Re-initialize the form whenever the dialog opens (user interaction), so
  // it always reflects the target year.
  useDialogReset(() => open, syncFromEntry);

  function syncFromEntry() {
    amount = initialAmount;
    formError = '';
    loading = false;
  }

  async function handleSave() {
    if (amount <= 0) return;
    loading = true;
    formError = '';
    try {
      await setBudgetYearAmount(accountGuid, year, amount);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save annual budget');
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
      <Dialog.Title>Set Budget</Dialog.Title>
      <Dialog.Description>
        {exists
          ? 'Adjust the annual target for this year.'
          : 'Create the annual target for this year.'}
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
            aria-invalid={!!amountError}
            aria-describedby={amountError ? 'amount-error' : undefined}
            autofocus
          />
          <span
            class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-muted-foreground uppercase"
            >{currency}</span
          >
        </div>
        {#if amountError}
          <p id="amount-error" role="alert" class="text-xs text-destructive">{amountError}</p>
        {/if}
        {#if formError}
          <p role="alert" class="text-sm text-destructive">{formError}</p>
        {/if}
      </div>
    </div>

    <SaveFooter
      mode="create"
      saving={loading}
      deleting={false}
      confirmDelete={false}
      saveLabel={exists ? 'Save' : 'Create budget'}
      {saveDisabled}
      onDelete={() => {}}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
