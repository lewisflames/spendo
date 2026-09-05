<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';

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
  let amountError = $state('');

  // Re-initialize the form whenever the dialog opens (user interaction), so
  // it always reflects the target year.
  function syncFromEntry() {
    amount = initialAmount;
    amountError = '';
  }

  async function handleSave() {
    amountError = amount > 0 ? '' : 'Target must be greater than zero';
    if (amount <= 0) return;
    loading = true;
    try {
      await setBudgetYearAmount(accountGuid, year, amount);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to save annual budget'));
    } finally {
      loading = false;
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={(o) => o && syncFromEntry()}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{exists ? `Edit ${year} Budget` : `Set ${year} Budget`}</Dialog.Title>
      <Dialog.Description>
        {exists
          ? 'Adjust the annual target for this year.'
          : 'Create the annual target for this year.'}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-2">
        <span class="text-sm font-medium">Year</span>
        <p class="text-sm">{year}</p>
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
    </div>

    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button onclick={handleSave} disabled={loading}>
        {loading ? 'Saving...' : exists ? 'Save' : 'Create Budget'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
