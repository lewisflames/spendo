<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Input } from '$lib/components/ui/input';
  import DatePicker from '$lib/components/shared/date-picker/date-picker.svelte';
  import { formatDate, plural } from '$lib/utils/format.svelte';

  let {
    statementDate = $bindable(),
    endingBalanceInput = $bindable(),
    balanceIssue,
    checkedCount,
    totalCount,
    dirty,
    balanced,
    lastReconciledDate,
    maxDate
  }: {
    statementDate: string;
    endingBalanceInput: string;
    /** Why the ending balance can’t be used yet (drives the hint + aria state). */
    balanceIssue: 'empty' | 'invalid' | 'none';
    checkedCount: number;
    totalCount: number;
    dirty: boolean;
    balanced: boolean;
    lastReconciledDate: string | null;
    maxDate: string;
  } = $props();
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
  </Card.Content>
</Card.Root>
