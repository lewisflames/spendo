<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import DatePicker from '$lib/components/shared/date-picker/date-picker.svelte';
  import { formatSessionCurrency as fmt, signColor, EMPTY_DASH } from '$lib/utils/format.svelte';
  import { plural } from '$lib/utils/format.svelte';

  let {
    statementDate = $bindable(),
    endingBalanceInput = $bindable(),
    startingBalance,
    cleared,
    target,
    difference,
    balanced,
    checkedCount,
    totalCount,
    dirty,
    saving,
    finishing,
    canFinish,
    onSave,
    onFinish
  }: {
    statementDate: string;
    endingBalanceInput: string;
    startingBalance: number;
    cleared: number;
    target: number;
    difference: number;
    balanced: boolean;
    checkedCount: number;
    totalCount: number;
    dirty: boolean;
    saving: boolean;
    finishing: boolean;
    canFinish: boolean;
    onSave: () => void;
    onFinish: () => void;
  } = $props();

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
    </Card.Description>
  </Card.Header>
  <Card.Content class="flex flex-col gap-4">
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <div class="flex flex-col gap-1.5">
        <label for="reconcile-statement-date" class="text-sm font-medium">Statement date</label>
        <DatePicker
          id="reconcile-statement-date"
          bind:value={statementDate}
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
          bind:value={endingBalanceInput}
        />
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
        <dd class="font-medium tabular-nums {differenceClass}">
          {Number.isFinite(difference) ? fmt(difference) : EMPTY_DASH}
        </dd>
      </div>
    </dl>
    <div class="flex flex-wrap items-center gap-2">
      <Button variant="outline" disabled={!dirty || saving || finishing} onclick={onSave}>
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
          You’ve got unsaved changes. Save now, or we’ll save them when you finish.
        </p>
      {:else if !balanced}
        <p class="text-sm text-muted-foreground">
          Get the difference to zero to finish. Tick off what cleared the bank.
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
