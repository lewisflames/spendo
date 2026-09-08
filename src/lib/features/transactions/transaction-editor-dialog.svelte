<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import DialogSaveFooter from '$lib/components/shared/dialog-save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { Plus, Trash2 } from '@lucide/svelte';
  import {
    createTransaction,
    deleteTransaction,
    fetchAccounts,
    updateTransaction,
    type AccountsResult,
    type TransactionDetail
  } from '$lib/tauri';
  import type { TxInput } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { isFutureDate, isIsoDate, todayStr } from '$lib/utils/date-utils.svelte';
  import { formatSessionCurrency } from '$lib/utils/format.svelte';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';

  interface SplitDraft {
    key: number;
    accountGuid: string;
    amount: string;
    quantity: string;
    showQuantity: boolean;
    memo: string;
  }

  let {
    open = $bindable(false),
    mode = 'create',
    guid = null,
    initial = null,
    onSaved,
    onDeleted,
    onError
  }: {
    open: boolean;
    mode?: 'create' | 'edit';
    guid?: string | null;
    initial?: TransactionDetail | null;
    onSaved?: (guid: string) => void;
    onDeleted?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let keySeq = $state(0);
  let postDate = $state(todayStr());
  let description = $state('');
  let num = $state('');
  let notes = $state('');
  let splits = $state<SplitDraft[]>([]);
  let saving = $state(false);
  let deleting = $state(false);
  let confirmDelete = $state(false);
  let formError = $state('');

  let accountsData = $state<AccountsResult | null>(null);
  let accountsLoading = $state(false);
  let accountsError = $state('');

  let pickerOptions = $derived(
    flattenAccountsForPicker(accountsData?.trees, { hidePlaceholders: true })
  );

  function emptySplit(): SplitDraft {
    keySeq += 1;
    return {
      key: keySeq,
      accountGuid: '',
      amount: '',
      quantity: '',
      showQuantity: false,
      memo: ''
    };
  }

  function syncDefaults() {
    formError = '';
    confirmDelete = false;
    saving = false;
    deleting = false;
    if (mode === 'edit' && initial) {
      postDate = initial.postDate.slice(0, 10);
      description = initial.description ?? '';
      num = initial.num ?? '';
      notes = initial.notes ?? '';
      splits = initial.splits.map((s) => {
        keySeq += 1;
        const qtyDiffers =
          s.quantity !== 0 && Math.abs(Math.abs(s.quantity) - Math.abs(s.amount)) >= 1e-6;
        return {
          key: keySeq,
          accountGuid: s.accountGuid,
          amount: String(s.amount),
          quantity: qtyDiffers ? String(s.quantity) : '',
          showQuantity: qtyDiffers,
          memo: s.memo ?? ''
        };
      });
      if (splits.length < 2) {
        while (splits.length < 2) splits.push(emptySplit());
      }
    } else {
      postDate = todayStr();
      description = '';
      num = '';
      notes = '';
      splits = [emptySplit(), emptySplit()];
    }
  }

  useDialogReset(() => open, syncDefaults);

  // Load account trees for the picker when the dialog opens.
  $effect(() => {
    if (!open) return;
    let cancelled = false;
    accountsLoading = true;
    accountsError = '';
    fetchAccounts()
      .then((r) => {
        if (cancelled) return;
        accountsData = r;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        accountsData = null;
        accountsError = errMsg(e, 'Failed to load accounts');
      })
      .finally(() => {
        if (!cancelled) accountsLoading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  function parseAmount(raw: string): number | null {
    const t = raw.trim();
    if (!t) return null;
    const n = Number(t);
    return Number.isFinite(n) ? n : null;
  }

  let dateError = $derived.by(() => {
    if (!postDate) return 'Date is required';
    if (!isIsoDate(postDate)) return 'Use a valid date (YYYY-MM-DD)';
    if (isFutureDate(postDate)) return 'Future dates are not allowed';
    return '';
  });

  let parsedAmounts = $derived(splits.map((s) => parseAmount(s.amount)));

  let balance = $derived.by(() => {
    let sum = 0;
    for (const n of parsedAmounts) {
      if (n === null) return null;
      sum += n;
    }
    return sum;
  });

  let balanced = $derived(balance !== null && Math.abs(balance) < 1e-6);

  let splitErrors = $derived(
    splits.map((s, i) => {
      if (!s.accountGuid) return `Split ${i + 1}: pick an account`;
      if (parseAmount(s.amount) === null) return `Split ${i + 1}: enter a number`;
      if (s.showQuantity && s.quantity.trim() && parseAmount(s.quantity) === null)
        return `Split ${i + 1}: quantity must be a number`;
      return '';
    })
  );

  let canSave = $derived(
    !saving &&
      !deleting &&
      !dateError &&
      splits.length >= 2 &&
      splitErrors.every((e) => !e) &&
      balanced
  );

  function buildInput(): TxInput {
    return {
      postDate,
      description: description.trim() ? description.trim() : null,
      num: num.trim() ? num.trim() : null,
      notes: notes.trim() ? notes.trim() : null,
      splits: splits.map((s) => ({
        accountGuid: s.accountGuid,
        amount: parseAmount(s.amount) ?? 0,
        quantity: s.showQuantity && s.quantity.trim() ? parseAmount(s.quantity) : null,
        memo: s.memo.trim() ? s.memo.trim() : null
      }))
    };
  }

  async function handleSave() {
    if (!canSave) return;
    saving = true;
    formError = '';
    try {
      const input = buildInput();
      if (mode === 'edit' && guid) {
        await updateTransaction(guid, input);
        open = false;
        onSaved?.(guid);
      } else {
        const newGuid = await createTransaction(input);
        open = false;
        onSaved?.(newGuid);
      }
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save transaction');
      formError = msg;
      onError?.(msg);
    } finally {
      saving = false;
    }
  }

  async function handleDelete() {
    if (mode !== 'edit' || !guid) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    deleting = true;
    formError = '';
    try {
      await deleteTransaction(guid);
      open = false;
      onDeleted?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to delete transaction');
      formError = msg;
      onError?.(msg);
    } finally {
      deleting = false;
    }
  }

  function addSplit() {
    if (splits.length >= 50) return;
    splits = [...splits, emptySplit()];
  }

  function removeSplit(key: number) {
    if (splits.length <= 2) return;
    splits = splits.filter((s) => s.key !== key);
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>{mode === 'edit' ? 'Edit transaction' : 'New transaction'}</Dialog.Title>
      <Dialog.Description>
        Splits use raw amounts as shown in the detail view and must sum to zero. Future dates are
        blocked so balances never diverge.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-2">
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="flex flex-col gap-1.5 sm:col-span-2">
          <label for="tx-desc" class="text-sm font-medium">Description</label>
          <Input
            id="tx-desc"
            bind:value={description}
            placeholder="e.g. Groceries"
            maxlength={500}
            autofocus
          />
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="tx-date" class="text-sm font-medium">Date</label>
          <DatePicker
            id="tx-date"
            bind:value={postDate}
            placeholder="Pick a date"
            invalid={!!dateError}
            describedBy={dateError ? 'tx-date-error' : undefined}
          />
          {#if dateError}
            <p id="tx-date-error" role="alert" class="text-xs text-destructive">{dateError}</p>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="tx-num" class="text-sm font-medium">Number (optional)</label>
          <Input id="tx-num" bind:value={num} placeholder="e.g. 2024-001" maxlength={100} />
        </div>
        <div class="flex flex-col gap-1.5 sm:col-span-2">
          <label for="tx-notes" class="text-sm font-medium">Notes (optional)</label>
          <Textarea
            id="tx-notes"
            bind:value={notes}
            placeholder="Optional notes"
            rows={2}
            maxlength={2000}
          />
        </div>
      </div>

      <div class="flex flex-col gap-3">
        <div class="flex items-center justify-between">
          <h3 class="text-sm font-medium">Splits</h3>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onclick={addSplit}
            disabled={splits.length >= 50}
          >
            <Plus aria-hidden="true" class="size-3" />
            Add split
          </Button>
        </div>

        {#if accountsError}
          <p role="alert" class="text-xs text-destructive">{accountsError}</p>
        {/if}

        {#each splits as s, i (s.key)}
          <div class="flex flex-col gap-2 rounded-lg border p-3">
            <div class="flex items-center justify-between gap-2">
              <span class="text-xs font-medium text-muted-foreground">Split {i + 1}</span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onclick={() => removeSplit(s.key)}
                disabled={splits.length <= 2}
                aria-label="Remove split {i + 1}"
              >
                <Trash2 aria-hidden="true" class="size-3" />
                Remove
              </Button>
            </div>
            <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
              <div class="flex flex-col gap-1.5 sm:col-span-2">
                <label for="split-{s.key}-account" class="text-xs text-muted-foreground"
                  >Account</label
                >
                <AccountPicker
                  id="split-{s.key}-account"
                  bind:value={s.accountGuid}
                  options={pickerOptions}
                  placeholder={accountsLoading ? 'Loading accounts…' : 'Select an account'}
                  disabled={accountsLoading}
                  invalid={!!splitErrors[i] && !s.accountGuid}
                  describedBy={splitErrors[i] ? `split-{s.key}-error` : undefined}
                />
                {#if splitErrors[i]}
                  <p id="split-{s.key}-error" role="alert" class="text-xs text-destructive">
                    {splitErrors[i]}
                  </p>
                {/if}
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="split-{s.key}-amount" class="text-xs text-muted-foreground"
                  >Amount</label
                >
                <Input
                  id="split-{s.key}-amount"
                  type="number"
                  step="0.01"
                  bind:value={s.amount}
                  placeholder="0.00"
                  aria-invalid={parseAmount(s.amount) === null}
                />
              </div>
              <div class="flex flex-col gap-1.5">
                {#if s.showQuantity}
                  <div class="flex items-center justify-between">
                    <label for="split-{s.key}-qty" class="text-xs text-muted-foreground"
                      >Quantity</label
                    >
                    <button
                      type="button"
                      class="text-xs text-muted-foreground underline-offset-2 hover:underline"
                      onclick={() => {
                        s.showQuantity = false;
                        s.quantity = '';
                      }}
                    >
                      Hide
                    </button>
                  </div>
                  <Input
                    id="split-{s.key}-qty"
                    type="number"
                    step="any"
                    bind:value={s.quantity}
                    placeholder="Shares (optional)"
                  />
                {:else}
                  <span class="text-xs text-muted-foreground">Quantity</span>
                  <button
                    type="button"
                    class="h-8 rounded-lg border border-dashed px-2.5 text-left text-sm text-muted-foreground hover:bg-muted/50"
                    onclick={() => {
                      s.showQuantity = true;
                    }}
                  >
                    + shares (investments)
                  </button>
                {/if}
              </div>
              <div class="flex flex-col gap-1.5 sm:col-span-2">
                <label for="split-{s.key}-memo" class="text-xs text-muted-foreground"
                  >Memo (optional)</label
                >
                <Input
                  id="split-{s.key}-memo"
                  bind:value={s.memo}
                  placeholder="Split memo"
                  maxlength={500}
                />
              </div>
            </div>
            {#if splitErrors[i]}
              <p role="alert" class="text-xs text-destructive">{splitErrors[i]}</p>
            {/if}
          </div>
        {/each}

        <div
          class="rounded-lg border px-3 py-2 text-sm {balanced
            ? 'border-success/40 bg-success/10 text-success'
            : 'border-destructive/40 bg-destructive/10 text-destructive'}"
          role="status"
        >
          {#if balance === null}
            Enter an amount for every split.
          {:else if balanced}
            Balanced · splits sum to zero.
          {:else}
            Out of balance by {formatSessionCurrency(balance)} — debits must equal credits.
          {/if}
        </div>
      </div>

      {#if formError}
        <p role="alert" class="text-sm text-destructive">{formError}</p>
      {/if}

      {#if mode === 'edit' && confirmDelete}
        <p
          role="alert"
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          Delete this transaction? Its splits will be removed. This can be undone from history
          (newest-first).
        </p>
      {/if}
    </div>

    <DialogSaveFooter
      {mode}
      {saving}
      {deleting}
      {confirmDelete}
      saveLabel={mode === 'edit' ? 'Save changes' : 'Create transaction'}
      saveDisabled={!canSave}
      onDelete={handleDelete}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
