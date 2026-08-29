<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';

  import { setBudgetAmount, resetBudgetAmount } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { appSettings } from '$lib/stores/settings.svelte';
  import type { BudgetEntry } from '$lib/tauri';

  let {
    open = $bindable(false),
    accountGuid,
    entry,
    currency = 'EUR',
    onSave,
    onError
  }: {
    open: boolean;
    accountGuid: string;
    entry: BudgetEntry;
    currency?: string;
    onSave?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let amount = $state(0);

  let loading = $state(false);
  let amountError = $state('');

  let periodLabel = $derived(
    new Date(entry.year, entry.month - 1, 1).toLocaleDateString(appSettings.value.locale, {
      month: 'long',
      year: 'numeric'
    })
  );

  // Re-initialize the form whenever the dialog opens (user interaction), so
  // it always reflects the target entry.
  function syncFromEntry() {
    amount = entry.amount;
    amountError = '';
  }

  async function handleSave() {
    amountError = amount > 0 ? '' : 'Target must be greater than zero';
    if (amount <= 0) return;
    loading = true;
    try {
      await setBudgetAmount(accountGuid, entry.year, entry.month, amount);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to save budget amount'));
    } finally {
      loading = false;
    }
  }

  async function handleReset() {
    loading = true;
    try {
      await resetBudgetAmount(accountGuid, entry.year, entry.month);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to reset budget amount'));
    } finally {
      loading = false;
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={(o) => o && syncFromEntry()}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Edit Budget Amount</Dialog.Title>
      <Dialog.Description>Adjust the target for this period.</Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-2">
        <span class="text-sm font-medium">Period</span>
        <p class="text-sm capitalize">{periodLabel}</p>
      </div>

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
    </div>

    <Dialog.Footer>
      <Button
        variant="ghost"
        class="mr-auto text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
        onclick={handleReset}
        disabled={loading}
      >
        Reset to default
      </Button>
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button onclick={handleSave} disabled={loading}>
        {loading ? 'Saving...' : 'Save'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
