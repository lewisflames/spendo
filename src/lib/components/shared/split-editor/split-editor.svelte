<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { Plus, Trash2, X } from '@lucide/svelte';
  import type { PickerAccount } from '$lib/features/accounts/account-picker-utils';
  import { formatSessionCurrency } from '$lib/utils/format.svelte';
  import {
    isBalanced,
    makeSplitDraft,
    parseAmount,
    splitBalance,
    splitErrors,
    type SplitDraft
  } from './split-draft';

  let {
    splits = $bindable<SplitDraft[]>(),
    options = [],
    loading = false,
    loadError = '',
    title = 'Splits',
    addLabel = 'Add split',
    showErrors = false,
    makeKey,
    idPrefix = 'split'
  }: {
    splits?: SplitDraft[];
    options?: PickerAccount[];
    loading?: boolean;
    loadError?: string;
    title?: string;
    addLabel?: string;
    showErrors?: boolean;
    makeKey: () => number;
    idPrefix?: string;
  } = $props();

  let errors = $derived(splitErrors(splits));
  let balance = $derived(splitBalance(splits));
  let balanced = $derived(isBalanced(balance));

  function addSplit() {
    if (splits.length >= 50) return;
    splits = [...splits, makeSplitDraft(makeKey())];
  }

  function removeSplit(key: number) {
    if (splits.length <= 2) return;
    splits = splits.filter((s) => s.key !== key);
  }
</script>

<div class="flex flex-col gap-3">
  <div class="flex items-center justify-between">
    <h3 class="text-sm font-medium">{title}</h3>
    <Button
      type="button"
      variant="outline"
      size="sm"
      onclick={addSplit}
      disabled={splits.length >= 50}
    >
      <Plus aria-hidden="true" class="size-3" />
      {addLabel}
    </Button>
  </div>

  {#if loadError}
    <p role="alert" class="text-xs text-destructive">{loadError}</p>
  {/if}

  <div class="overflow-hidden rounded-lg border">
    <div
      class="hidden gap-2 border-b bg-muted/50 px-3 py-2 text-xs font-medium text-muted-foreground sm:grid sm:grid-cols-[minmax(0,2fr)_6.5rem_6rem_minmax(0,1.5fr)_2rem]"
    >
      <span>Account</span>
      <span>Amount</span>
      <span>Quantity</span>
      <span>Memo</span>
      <span class="sr-only">Actions</span>
    </div>

    <div class="divide-y">
      {#each splits as s, i (s.key)}
        <div
          class="grid grid-cols-1 gap-2 p-3 sm:grid-cols-[minmax(0,2fr)_6.5rem_6rem_minmax(0,1.5fr)_2rem] sm:items-center sm:gap-2 sm:px-3 sm:py-2"
        >
          <AccountPicker
            id="{idPrefix}-{s.key}-account"
            bind:value={s.accountGuid}
            {options}
            placeholder={loading ? 'Loading accounts…' : 'Select an account'}
            disabled={loading}
            invalid={showErrors && !!errors[i] && !s.accountGuid}
            describedBy={showErrors && errors[i] ? `${idPrefix}-${s.key}-error` : undefined}
          />
          <Input
            id="{idPrefix}-{s.key}-amount"
            type="text"
            inputmode="decimal"
            bind:value={s.amount}
            placeholder="0.00"
            aria-label="Split {i + 1} amount"
            aria-invalid={showErrors && parseAmount(s.amount) === null}
          />
          <div class="flex items-center gap-1">
            {#if s.showQuantity}
              <Input
                id="{idPrefix}-{s.key}-qty"
                type="text"
                inputmode="decimal"
                bind:value={s.quantity}
                placeholder="Qty"
                aria-label="Split {i + 1} quantity"
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                class="size-8 shrink-0"
                aria-label="Hide quantity for split {i + 1}"
                title="Hide quantity"
                onclick={() => {
                  s.showQuantity = false;
                  s.quantity = '';
                }}
              >
                <X aria-hidden="true" class="size-3.5" />
              </Button>
            {:else}
              <Button
                type="button"
                variant="ghost"
                size="icon"
                class="size-8 shrink-0 text-muted-foreground"
                aria-label="Add quantity to split {i + 1}"
                title="Add quantity (investments)"
                onclick={() => {
                  s.showQuantity = true;
                }}
              >
                <Plus aria-hidden="true" class="size-3.5" />
              </Button>
            {/if}
          </div>
          <Input
            id="{idPrefix}-{s.key}-memo"
            bind:value={s.memo}
            placeholder="Memo"
            maxlength={500}
            aria-label="Split {i + 1} memo"
          />
          <div class="flex justify-end sm:justify-center">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              class="size-8 text-muted-foreground"
              onclick={() => removeSplit(s.key)}
              disabled={splits.length <= 2}
              aria-label="Remove split {i + 1}"
              title="Remove split"
            >
              <Trash2 aria-hidden="true" class="size-3.5" />
            </Button>
          </div>
          {#if showErrors && errors[i]}
            <p
              id="{idPrefix}-{s.key}-error"
              role="alert"
              class="text-xs text-destructive sm:col-span-full"
            >
              {errors[i]}
            </p>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  <div
    class="rounded-lg border px-3 py-2 text-sm {balanced
      ? 'border-success/40 bg-success/10 text-success'
      : showErrors || balance !== null
        ? 'border-destructive/40 bg-destructive/10 text-destructive'
        : 'text-muted-foreground'}"
    role="status"
  >
    {#if balance === null}
      {showErrors ? 'Enter an amount for every split.' : 'Splits must sum to zero.'}
    {:else if balanced}
      Balanced · splits sum to zero.
    {:else}
      Out of balance by {formatSessionCurrency(balance)} — debits must equal credits.
    {/if}
  </div>
</div>
