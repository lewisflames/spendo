<script lang="ts">
  import { tick } from 'svelte';
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
    autoFillIndex,
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

  // Exactly one row still needs an amount: offer to fill it (button, ghost
  // placeholder and Enter) instead of making users hunt for the scale icon.
  let autoFill = $derived(autoFillIndex(splits));
  let showQtyColumn = $derived(splits.some((s) => s.showQuantity));
  // Tailwind sees both class strings in this file, so either grid is generated.
  let gridCols = $derived(
    showQtyColumn
      ? '@xl:grid-cols-[minmax(0,2fr)_9rem_7rem_minmax(0,1.5fr)_auto]'
      : '@xl:grid-cols-[minmax(0,2fr)_9rem_minmax(0,1.5fr)_auto]'
  );

  let typeByGuid = $derived(new Map(options.map((o) => [o.guid, o.accountType])));
  let labelByGuid = $derived(new Map(options.map((o) => [o.guid, o.display])));
  let commodityByGuid = $derived(new Map(options.map((o) => [o.guid, o.commodity])));

  // Auto-reveal quantity when the picked account carries shares. Tracked per
  // row so a deliberate hide isn’t re-opened on every render.
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

  function focusElement(id: string) {
    void tick().then(() => {
      document.getElementById(id)?.focus();
    });
  }

  function focusAmount(key: number) {
    focusElement(`${idPrefix}-${key}-amount`);
  }

  function addSplit() {
    if (splits.length >= 50) return;
    const key = makeKey();
    splits = [...splits, makeSplitDraft(key)];
    focusElement(`${idPrefix}-${key}-account`);
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

  /**
   * Enter accepts the remainder once this is the only unfilled row; otherwise
   * it hops to the next row that still needs an amount.
   */
  function handleAmountKeydown(e: KeyboardEvent, index: number) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    if (autoFill === index) {
      const remainder = remainderFor(index, splits);
      if (remainder !== null) {
        splits[index].amount = formatDraftAmount(remainder);
        return;
      }
    }
    for (let j = index + 1; j < splits.length; j += 1) {
      if (parseAmount(splits[j].amount) === null) {
        focusAmount(splits[j].key);
        return;
      }
    }
  }

  /** Unit price for investment rows once both amount and share count parse. */
  function unitPrice(s: SplitDraft): string | null {
    if (!s.showQuantity) return null;
    const amount = parseAmount(s.amount);
    const quantity = parseAmount(s.quantity);
    if (amount === null || quantity === null || Math.abs(quantity) < 1e-9) return null;
    const commodity = commodityByGuid.get(s.accountGuid);
    const price = formatSessionCurrency(Math.abs(amount / quantity));
    return commodity ? `${price} / ${commodity}` : price;
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
      class="hidden gap-2 border-b bg-muted/50 px-3 py-2 text-xs font-medium text-muted-foreground @xl:grid {gridCols}"
    >
      <span>Account</span>
      <span>Amount</span>
      {#if showQtyColumn}
        <span>Quantity</span>
      {/if}
      <span>Memo</span>
      <span class="sr-only">Row actions</span>
    </div>

    <div class="divide-y">
      {#each splits as s, i (s.key)}
        {@const label = rowLabel(s, i)}
        {@const remainder = remainderFor(i, splits)}
        {@const price = unitPrice(s)}
        <div
          class="grid grid-cols-2 gap-2 p-3 @xl:items-center @xl:gap-2 @xl:px-3 @xl:py-2 {gridCols}"
        >
          <div class="col-span-2 @xl:col-span-1">
            <AccountPicker
              id="{idPrefix}-{s.key}-account"
              bind:value={s.accountGuid}
              {options}
              placeholder={loading ? 'Loading accounts…' : 'Select an account'}
              disabled={loading}
              invalid={showErrors && !!errors[i] && !s.accountGuid}
              describedBy={showErrors && errors[i] ? `${idPrefix}-${s.key}-error` : undefined}
              onSelect={() => focusAmount(s.key)}
            />
          </div>
          <div class="col-span-1 flex flex-col gap-1">
            <Input
              id="{idPrefix}-{s.key}-amount"
              type="text"
              inputmode="decimal"
              bind:value={s.amount}
              placeholder={autoFill === i && remainder !== null
                ? formatDraftAmount(remainder)
                : '0.00'}
              title="Accepts amounts like 1,000.00, $12.50 or 10+5"
              aria-label="{label} amount"
              aria-invalid={showErrors && parseAmount(s.amount) === null}
              onkeydown={(e) => handleAmountKeydown(e, i)}
            />
            {#if price}
              <p class="text-[11px] text-muted-foreground tabular-nums">{price}</p>
            {/if}
          </div>
          {#if showQtyColumn}
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
              {/if}
            </div>
          {/if}
          <div class="col-span-2 @xl:col-span-1">
            <Input
              id="{idPrefix}-{s.key}-memo"
              bind:value={s.memo}
              placeholder="Memo"
              maxlength={500}
              aria-label="{label} memo"
            />
          </div>
          <div class="col-span-2 flex justify-end gap-0.5 @xl:col-span-1 @xl:justify-center">
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
                {#if s.showQuantity}
                  <DropdownMenu.Item
                    onclick={() => {
                      s.showQuantity = false;
                      s.quantity = '';
                    }}
                  >
                    <X aria-hidden="true" />
                    Remove quantity
                  </DropdownMenu.Item>
                {:else}
                  <DropdownMenu.Item onclick={() => (s.showQuantity = true)}>
                    <Plus aria-hidden="true" />
                    Add quantity
                  </DropdownMenu.Item>
                {/if}
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
              class="col-span-2 text-xs text-destructive @xl:col-span-full"
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
    {#if autoFill >= 0}
      <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span class="tabular-nums">
          One split left to fill. Row {autoFill + 1} balances with {formatSessionCurrency(
            remainderFor(autoFill, splits) ?? 0
          )}.
        </span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          class="h-6 px-2"
          onclick={() => fillRemainder(autoFill)}
        >
          Fill row {autoFill + 1}
        </Button>
      </div>
    {:else if totals}
      {#if balanced}
        <span class="tabular-nums">Balanced · {formatSessionCurrency(totals.credit)}</span>
      {:else}
        <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span class="tabular-nums">
            Money out {formatSessionCurrency(totals.debit)} · Money in {formatSessionCurrency(
              totals.credit
            )}
          </span>
          <span class="tabular-nums">· Off by {formatSessionCurrency(Math.abs(balance ?? 0))}</span>
        </div>
      {/if}
    {:else if showErrors}
      Add an amount to every split.
    {:else}
      Splits should net to zero. Try 1,000.00 or even 10+5.
    {/if}
  </div>
</div>
