<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import {
    ArrowRightLeft,
    ChevronDown,
    ChevronUp,
    Copy,
    Ellipsis,
    Plus,
    Scale,
    Trash2,
    X
  } from '@lucide/svelte';
  import type { PickerAccount } from '$lib/features/accounts/account-picker-utils';
  import { formatSessionCurrency } from '$lib/utils/format.svelte';
  import {
    formatDraftAmount,
    isBalanced,
    isInvestmentAccountType,
    makeSplitDraft,
    parseAmount,
    remainderFor,
    splitBalance,
    splitErrors,
    splitTotals,
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
  let totals = $derived(splitTotals(splits));

  let typeByGuid = $derived(new Map(options.map((o) => [o.guid, o.accountType])));
  let labelByGuid = $derived(new Map(options.map((o) => [o.guid, o.display])));

  // Auto-reveal quantity when the picked account carries shares. Tracked per
  // row so a deliberate hide isn't re-opened on every render.
  let prevAccounts: { key: number; accountGuid: string }[] = [];
  $effect(() => {
    for (const s of splits) {
      const prev = prevAccounts.find((p) => p.key === s.key);
      if (prev?.accountGuid === s.accountGuid) continue;
      if (prev) prev.accountGuid = s.accountGuid;
      else prevAccounts.push({ key: s.key, accountGuid: s.accountGuid });
      if (
        s.accountGuid &&
        !s.showQuantity &&
        isInvestmentAccountType(typeByGuid.get(s.accountGuid))
      ) {
        s.showQuantity = true;
      }
    }
    prevAccounts = prevAccounts.filter((p) => splits.some((s) => s.key === p.key));
  });

  function rowLabel(s: SplitDraft, i: number): string {
    return labelByGuid.get(s.accountGuid) ?? `Split ${i + 1}`;
  }

  function addSplit() {
    if (splits.length >= 50) return;
    splits = [...splits, makeSplitDraft(makeKey())];
  }

  function removeSplit(key: number) {
    if (splits.length <= 2) return;
    splits = splits.filter((s) => s.key !== key);
  }

  function duplicateSplit(key: number) {
    if (splits.length >= 50) return;
    const idx = splits.findIndex((s) => s.key === key);
    if (idx < 0) return;
    const src = splits[idx];
    const copy: SplitDraft = { ...src, key: makeKey() };
    splits = [...splits.slice(0, idx + 1), copy, ...splits.slice(idx + 1)];
  }

  function moveSplit(key: number, dir: -1 | 1) {
    const idx = splits.findIndex((s) => s.key === key);
    const next = idx + dir;
    if (idx < 0 || next < 0 || next >= splits.length) return;
    const copy = [...splits];
    [copy[idx], copy[next]] = [copy[next], copy[idx]];
    splits = copy;
  }

  function negateAmount(s: SplitDraft) {
    const n = parseAmount(s.amount);
    if (n === null) return;
    s.amount = formatDraftAmount(-n);
  }

  function fillRemainder(i: number) {
    const r = remainderFor(i, splits);
    if (r === null) return;
    splits[i].amount = formatDraftAmount(r);
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
      <Plus aria-hidden="true" class="size-3.5" />
      {addLabel}
    </Button>
  </div>

  {#if loadError}
    <p role="alert" class="text-xs text-destructive">{loadError}</p>
  {/if}

  <div class="overflow-hidden rounded-lg border">
    <div
      class="hidden gap-2 border-b bg-muted/50 px-3 py-2 text-xs font-medium text-muted-foreground sm:grid sm:grid-cols-[minmax(0,2fr)_8rem_7rem_minmax(0,1.5fr)_auto]"
    >
      <span>Account</span>
      <span>Amount</span>
      <span>Quantity</span>
      <span>Memo</span>
      <span class="sr-only">Row actions</span>
    </div>

    <div class="divide-y">
      {#each splits as s, i (s.key)}
        {@const label = rowLabel(s, i)}
        {@const remainder = remainderFor(i, splits)}
        <div
          class="grid grid-cols-2 gap-2 p-3 sm:grid-cols-[minmax(0,2fr)_8rem_7rem_minmax(0,1.5fr)_auto] sm:items-center sm:gap-2 sm:px-3 sm:py-2"
        >
          <div class="col-span-2 sm:col-span-1">
            <AccountPicker
              id="{idPrefix}-{s.key}-account"
              bind:value={s.accountGuid}
              {options}
              placeholder={loading ? 'Loading accounts…' : 'Select an account'}
              disabled={loading}
              invalid={showErrors && !!errors[i] && !s.accountGuid}
              describedBy={showErrors && errors[i] ? `${idPrefix}-${s.key}-error` : undefined}
            />
          </div>
          <Input
            id="{idPrefix}-{s.key}-amount"
            type="text"
            inputmode="decimal"
            bind:value={s.amount}
            placeholder="0.00, 1,000.00 or 10+5"
            aria-label="{label} amount"
            aria-invalid={showErrors && parseAmount(s.amount) === null}
            class="col-span-1"
          />
          <div class="col-span-1 flex items-center gap-1">
            {#if s.showQuantity}
              <Input
                id="{idPrefix}-{s.key}-qty"
                type="text"
                inputmode="decimal"
                bind:value={s.quantity}
                placeholder="Qty"
                aria-label="{label} quantity"
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                class="size-8 shrink-0"
                aria-label="Hide quantity for {label}"
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
                aria-label="Add quantity to {label}"
                title="Add quantity (investments)"
                onclick={() => {
                  s.showQuantity = true;
                }}
              >
                <Plus aria-hidden="true" class="size-3.5" />
              </Button>
            {/if}
          </div>
          <div class="col-span-2 sm:col-span-1">
            <Input
              id="{idPrefix}-{s.key}-memo"
              bind:value={s.memo}
              placeholder="Memo"
              maxlength={500}
              aria-label="{label} memo"
            />
          </div>
          <div class="col-span-2 flex justify-end gap-0.5 sm:col-span-1 sm:justify-center">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              class="size-7 text-muted-foreground"
              onclick={() => fillRemainder(i)}
              disabled={remainder === null}
              aria-label="Fill {label} to balance"
              title="Fill to balance"
            >
              <Scale aria-hidden="true" class="size-3.5" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              class="size-7 text-muted-foreground"
              onclick={() => removeSplit(s.key)}
              disabled={splits.length <= 2}
              aria-label="Remove {label}"
              title="Remove split"
            >
              <Trash2 aria-hidden="true" class="size-3.5" />
            </Button>
            <DropdownMenu.Root>
              <DropdownMenu.Trigger>
                {#snippet child({ props })}
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    class="size-7 text-muted-foreground"
                    {...props}
                    aria-label="More actions for {label}"
                    title="More actions"
                  >
                    <Ellipsis aria-hidden="true" class="size-3.5" />
                  </Button>
                {/snippet}
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end">
                <DropdownMenu.Item disabled={i === 0} onclick={() => moveSplit(s.key, -1)}>
                  <ChevronUp aria-hidden="true" />
                  Move up
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  disabled={i === splits.length - 1}
                  onclick={() => moveSplit(s.key, 1)}
                >
                  <ChevronDown aria-hidden="true" />
                  Move down
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  disabled={parseAmount(s.amount) === null}
                  onclick={() => negateAmount(s)}
                >
                  <ArrowRightLeft aria-hidden="true" />
                  Negate amount
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  disabled={splits.length >= 50}
                  onclick={() => duplicateSplit(s.key)}
                >
                  <Copy aria-hidden="true" />
                  Duplicate split
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </div>
          {#if showErrors && errors[i]}
            <p
              id="{idPrefix}-{s.key}-error"
              role="alert"
              class="col-span-2 text-xs text-destructive sm:col-span-full"
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
    {#if totals}
      <span class="tabular-nums">
        Debits {formatSessionCurrency(totals.debit)} · Credits {formatSessionCurrency(
          totals.credit
        )}
      </span>
      {#if balanced}
        <span> · Balanced.</span>
      {:else}
        <span>
          · Out of balance by {formatSessionCurrency(balance ?? 0)} — use the scale button to fill a row.
        </span>
      {/if}
    {:else if showErrors}
      Enter an amount for every split.
    {:else}
      Splits must sum to zero — amounts accept 1,000.00 and 10+5.
    {/if}
  </div>
</div>
