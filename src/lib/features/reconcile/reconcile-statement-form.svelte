<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import DatePicker from '$lib/components/shared/date-picker/date-picker.svelte';
  import {
    formatCurrency,
    formatDate,
    signColor,
    EMPTY_DASH,
    plural
  } from '$lib/utils/format.svelte';

  let {
    statementDate = $bindable(),
    endingBalanceInput = $bindable(),
    currency,
    startingBalance,
    cleared,
    target,
    difference,
    balanced,
    balanceIssue,
    checkedCount,
    totalCount,
    dirty,
    saving,
    finishing,
    canFinish,
    lastReconciledDate,
    maxDate,
    onSave,
    onFinish,
    onFindDifference
  }: {
    statementDate: string;
    endingBalanceInput: string;
    currency: string;
    startingBalance: number;
    cleared: number;
    target: number;
    difference: number;
    balanced: boolean;
    /** Why the ending balance can’t be used yet (drives the hint + aria state). */
    balanceIssue: 'empty' | 'invalid' | 'none';
    checkedCount: number;
    totalCount: number;
    dirty: boolean;
    saving: boolean;
    finishing: boolean;
    canFinish: boolean;
    lastReconciledDate: string | null;
    maxDate: string;
    onSave: () => void;
    onFinish: () => void;
    onFindDifference: () => void;
  } = $props();

  const fmt = (n: number) => formatCurrency(n, currency);

  // `difference` is NaN while the ending balance is empty or invalid; keep the
  // color class off in that case so `signColor` never sees a non-finite value.
  let differenceClass = $derived(
    Number.isFinite(difference) ? (balanced ? 'text-success' : signColor(difference)) : ''
  );

  // Finishing promotes every cleared split, so the button opens a confirmation
  // instead of committing straight away. Transactions stay editable later:
  // saving one asks before putting its splits back to New.
  let confirmFinishOpen = $state(false);
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Statement</Card.Title>
    <Card.Description>
      Type in the ending balance and date from your statement, then tick off what cleared the bank.
      {checkedCount} of {plural(totalCount, 'candidate')} cleared.
      {#if lastReconciledDate}
        Last reconciled through {formatDate(lastReconciledDate)}.
      {/if}
    </Card.Description>
  </Card.Header>
  <Card.Content class="flex flex-col gap-4">
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <div class="flex flex-col gap-1.5">
        <label for="reconcile-statement-date" class="text-sm font-medium">Statement date</label>
        <DatePicker
          id="reconcile-statement-date"
          bind:value={statementDate}
          max={maxDate}
          placeholder="Pick a date"
        />
      </div>
      <div class="flex flex-col gap-1.5">
        <label for="reconcile-ending-balance" class="text-sm font-medium">Ending balance</label>
        <Input
          id="reconcile-ending-balance"
          type="text"
          inputmode="decimal"
          placeholder="0.00"
          aria-invalid={balanceIssue === 'invalid' ? true : undefined}
          aria-describedby={balanceIssue !== 'none' ? 'reconcile-ending-hint' : undefined}
          bind:value={endingBalanceInput}
        />
        {#if balanceIssue === 'invalid'}
          <p id="reconcile-ending-hint" class="text-xs text-destructive">
            Enter a number like 1,234.56.
          </p>
        {:else if balanceIssue === 'empty'}
          <p id="reconcile-ending-hint" class="text-xs text-muted-foreground">
            From your bank statement.
          </p>
        {/if}
      </div>
    </div>
    <dl class="grid grid-cols-2 gap-4 text-sm sm:grid-cols-4">
      <div>
        <dt class="text-muted-foreground">Starting (reconciled)</dt>
        <dd class="font-medium tabular-nums">{fmt(startingBalance)}</dd>
      </div>
      <div>
        <dt class="text-muted-foreground">Cleared</dt>
        <dd class="font-medium tabular-nums">{fmt(cleared)}</dd>
      </div>
      <div>
        <dt class="text-muted-foreground">Target</dt>
        <dd class="font-medium tabular-nums">{fmt(target)}</dd>
      </div>
      <div>
        <dt class="text-muted-foreground">Difference</dt>
        <dd class="font-medium tabular-nums {differenceClass}" aria-live="polite">
          {#if !Number.isFinite(difference)}
            {EMPTY_DASH}
          {:else if !balanced}
            <button
              type="button"
              class="rounded-sm underline decoration-dotted underline-offset-4 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
              title="Show the candidates matching this amount"
              onclick={onFindDifference}
            >
              {fmt(difference)}
            </button>
          {:else}
            {fmt(difference)}
          {/if}
        </dd>
      </div>
    </dl>
    <div class="flex flex-wrap items-center gap-2">
      <Button
        variant="outline"
        disabled={!dirty || saving || finishing}
        title="Save progress (Ctrl+S)"
        onclick={onSave}
      >
        {saving ? 'Saving…' : 'Save progress'}
      </Button>
      <Button
        disabled={!canFinish || saving || finishing}
        onclick={() => (confirmFinishOpen = true)}
      >
        {finishing ? 'Finishing…' : 'Finish reconciliation'}
      </Button>
      {#if dirty}
        <p class="text-sm text-muted-foreground">
          You’ve got unsaved changes. Save now, or we’ll save them when you finish. Ctrl+S saves.
        </p>
      {:else if balanceIssue === 'empty'}
        <p class="text-sm text-muted-foreground">
          Enter the ending balance from your bank statement.
        </p>
      {:else if balanceIssue === 'invalid'}
        <p class="text-sm text-muted-foreground">
          That balance doesn’t look like a number. Try something like 1,234.56.
        </p>
      {:else if checkedCount === 0}
        <p class="text-sm text-muted-foreground">Tick at least one transaction to finish.</p>
      {:else if !balanced}
        <p class="text-sm text-muted-foreground">
          The difference isn’t zero yet. Tick off what cleared the bank.
        </p>
      {/if}
    </div>
  </Card.Content>
</Card.Root>

<Dialog.Root bind:open={confirmFinishOpen}>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Finish reconciliation?</Dialog.Title>
      <Dialog.Description>
        This saves your progress and marks the {checkedCount} ticked splits as reconciled. Editing a transaction
        later will ask before putting its splits back to New.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (confirmFinishOpen = false)} disabled={finishing}>
        Cancel
      </Button>
      <Button
        disabled={finishing}
        onclick={() => {
          confirmFinishOpen = false;
          onFinish();
        }}
      >
        {finishing ? 'Finishing…' : 'Finish reconciliation'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
