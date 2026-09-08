<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import DialogSaveFooter from '$lib/components/shared/dialog-save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { Plus, Trash2 } from '@lucide/svelte';
  import {
    createSchedule,
    deleteSchedule,
    fetchAccounts,
    updateSchedule,
    type AccountsResult,
    type Schedule,
    type ScheduleFrequency,
    type ScheduleInput
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { formatSessionCurrency } from '$lib/utils/format.svelte';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { FREQUENCY_OPTIONS } from './schedule-utils';

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
    initial?: Schedule | null;
    onSaved?: (schedule: Schedule) => void;
    onDeleted?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let keySeq = $state(0);
  let name = $state('');
  let frequency = $state<ScheduleFrequency>('monthly');
  let intervalMult = $state('1');
  let startDate = $state(todayStr());
  let endDate = $state('');
  let maxOccurrences = $state('');
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
    accountsLoading = false;
    if (mode === 'edit' && initial) {
      name = initial.name;
      frequency = initial.frequency;
      intervalMult = String(initial.intervalMult);
      startDate = initial.startDate;
      endDate = initial.endDate ?? '';
      maxOccurrences = initial.maxOccurrences != null ? String(initial.maxOccurrences) : '';
      description = initial.description ?? '';
      num = initial.num ?? '';
      notes = initial.notes ?? '';
      splits = initial.splits.map((s) => {
        keySeq += 1;
        const qty = s.quantity ?? null;
        const qtyDiffers =
          qty !== null && qty !== 0 && Math.abs(Math.abs(qty) - Math.abs(s.amount)) >= 1e-6;
        return {
          key: keySeq,
          accountGuid: s.accountGuid,
          amount: String(s.amount),
          quantity: qtyDiffers && qty !== null ? String(qty) : '',
          showQuantity: qtyDiffers,
          memo: s.memo ?? ''
        };
      });
      if (splits.length < 2) {
        while (splits.length < 2) splits.push(emptySplit());
      }
    } else {
      name = '';
      frequency = 'monthly';
      intervalMult = '1';
      startDate = todayStr();
      endDate = '';
      maxOccurrences = '';
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

  function isIsoDate(v: string): boolean {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(v)) return false;
    const [y, m, d] = v.split('-').map(Number);
    const dt = new Date(y, m - 1, d);
    return dt.getFullYear() === y && dt.getMonth() === m - 1 && dt.getDate() === d;
  }

  let nameError = $derived.by(() => {
    if (!name.trim()) return 'Name is required';
    if (name.trim().length > 200) return 'Name is too long (max 200)';
    return '';
  });

  let multError = $derived.by(() => {
    const n = Number(intervalMult);
    if (!Number.isInteger(n) || n < 1 || n > 100) return 'Enter a whole number from 1 to 100';
    return '';
  });

  let startError = $derived.by(() => {
    if (!startDate) return 'Start date is required';
    if (!isIsoDate(startDate)) return 'Use YYYY-MM-DD';
    return '';
  });

  let endError = $derived.by(() => {
    if (!endDate) return '';
    if (!isIsoDate(endDate)) return 'Use YYYY-MM-DD';
    if (!startError && endDate < startDate) return 'End date cannot be before the start date';
    return '';
  });

  let countError = $derived.by(() => {
    if (!maxOccurrences.trim()) return '';
    const n = Number(maxOccurrences);
    if (!Number.isInteger(n) || n < 1 || n > 1200) return 'Enter a whole number from 1 to 1200';
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
      !nameError &&
      !multError &&
      !startError &&
      !endError &&
      !countError &&
      splits.length >= 2 &&
      splitErrors.every((e) => !e) &&
      balanced
  );

  function buildInput(): ScheduleInput {
    return {
      name: name.trim(),
      frequency,
      intervalMult: Number(intervalMult),
      startDate,
      endDate: endDate ? endDate : null,
      maxOccurrences: maxOccurrences.trim() ? Number(maxOccurrences) : null,
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
        const updated = await updateSchedule(guid, input);
        open = false;
        onSaved?.(updated);
      } else {
        const created = await createSchedule(input);
        open = false;
        onSaved?.(created);
      }
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save schedule');
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
      await deleteSchedule(guid);
      open = false;
      onDeleted?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to delete schedule');
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
      <Dialog.Title
        >{mode === 'edit'
          ? 'Edit scheduled transaction'
          : 'New scheduled transaction'}</Dialog.Title
      >
      <Dialog.Description>
        Nothing posts automatically — each due occurrence surfaces as a reminder to post or skip.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-2">
      <div class="flex flex-col gap-1.5">
        <label for="sched-name" class="text-sm font-medium">Name</label>
        <Input id="sched-name" bind:value={name} placeholder="e.g. Monthly rent" maxlength={200} />
        {#if nameError}
          <p role="alert" class="text-xs text-destructive">{nameError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-3">
        <h3 class="text-sm font-medium">Recurrence</h3>
        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div class="flex flex-col gap-1.5">
            <label for="sched-freq" class="text-sm font-medium">Repeats</label>
            <Select.Root
              type="single"
              value={frequency}
              onValueChange={(v) => {
                if (FREQUENCY_OPTIONS.some((o) => o.value === v))
                  frequency = v as ScheduleFrequency;
              }}
            >
              <Select.Trigger id="sched-freq">
                <SelectPrimitive.Value placeholder="Select frequency" />
              </Select.Trigger>
              <Select.Content>
                {#each FREQUENCY_OPTIONS as o (o.value)}
                  <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
          <div class="flex flex-col gap-1.5">
            <label for="sched-mult" class="text-sm font-medium">Every</label>
            <div class="flex items-center gap-2">
              <Input
                id="sched-mult"
                type="number"
                min={1}
                max={100}
                step={1}
                bind:value={intervalMult}
                class="w-24"
              />
              <span class="text-sm text-muted-foreground">
                {frequency === 'daily'
                  ? 'day(s)'
                  : frequency === 'weekly'
                    ? 'week(s)'
                    : frequency === 'biweekly'
                      ? '× 2 weeks'
                      : frequency === 'monthly'
                        ? 'month(s)'
                        : frequency === 'quarterly'
                          ? 'quarter(s)'
                          : 'year(s)'}
              </span>
            </div>
            {#if multError}
              <p role="alert" class="text-xs text-destructive">{multError}</p>
            {/if}
          </div>
          <div class="flex flex-col gap-1.5">
            <label for="sched-start" class="text-sm font-medium">First due date</label>
            <DatePicker id="sched-start" bind:value={startDate} placeholder="Pick a date" />
            {#if startError}
              <p role="alert" class="text-xs text-destructive">{startError}</p>
            {/if}
          </div>
          <div class="flex flex-col gap-1.5">
            <label for="sched-end" class="text-sm font-medium">End date (optional)</label>
            <DatePicker id="sched-end" bind:value={endDate} placeholder="No end date" />
            {#if endError}
              <p role="alert" class="text-xs text-destructive">{endError}</p>
            {/if}
          </div>
          <div class="flex flex-col gap-1.5">
            <label for="sched-count" class="text-sm font-medium">Stop after (optional)</label>
            <div class="flex items-center gap-2">
              <Input
                id="sched-count"
                type="number"
                min={1}
                max={1200}
                step={1}
                bind:value={maxOccurrences}
                placeholder="No limit"
                class="w-32"
              />
              <span class="text-sm text-muted-foreground">occurrence(s)</span>
            </div>
            {#if countError}
              <p role="alert" class="text-xs text-destructive">{countError}</p>
            {/if}
          </div>
        </div>
        {#if mode === 'edit' && initial && initial.occurrencesPosted > 0}
          <p class="text-xs text-muted-foreground">
            Already posted {initial.occurrencesPosted === 1
              ? 'once'
              : `${initial.occurrencesPosted} times`} — changing the start date won't move the next due
            date.
          </p>
        {/if}
      </div>

      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="flex flex-col gap-1.5 sm:col-span-2">
          <label for="sched-desc" class="text-sm font-medium">Description</label>
          <Input
            id="sched-desc"
            bind:value={description}
            placeholder="e.g. Monthly rent"
            maxlength={500}
          />
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="sched-num" class="text-sm font-medium">Number (optional)</label>
          <Input id="sched-num" bind:value={num} placeholder="e.g. 2024-001" maxlength={100} />
        </div>
        <div class="flex flex-col gap-1.5 sm:col-span-2">
          <label for="sched-notes" class="text-sm font-medium">Notes (optional)</label>
          <Textarea
            id="sched-notes"
            bind:value={notes}
            placeholder="Optional notes"
            rows={2}
            maxlength={2000}
          />
        </div>
      </div>

      <div class="flex flex-col gap-3">
        <div class="flex items-center justify-between">
          <h3 class="text-sm font-medium">Template splits</h3>
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
                <label for="sched-split-{s.key}-account" class="text-xs text-muted-foreground"
                  >Account</label
                >
                <AccountPicker
                  id="sched-split-{s.key}-account"
                  bind:value={s.accountGuid}
                  options={pickerOptions}
                  placeholder={accountsLoading ? 'Loading accounts…' : 'Select an account'}
                  disabled={accountsLoading}
                  invalid={!!splitErrors[i] && !s.accountGuid}
                />
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="sched-split-{s.key}-amount" class="text-xs text-muted-foreground"
                  >Amount</label
                >
                <Input
                  id="sched-split-{s.key}-amount"
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
                    <label for="sched-split-{s.key}-qty" class="text-xs text-muted-foreground"
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
                    id="sched-split-{s.key}-qty"
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
                <label for="sched-split-{s.key}-memo" class="text-xs text-muted-foreground"
                  >Memo (optional)</label
                >
                <Input
                  id="sched-split-{s.key}-memo"
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
          Delete this schedule? Future reminders stop; already-posted transactions stay in the
          ledger.
        </p>
      {/if}
    </div>

    <DialogSaveFooter
      {mode}
      {saving}
      {deleting}
      {confirmDelete}
      saveLabel={mode === 'edit' ? 'Save changes' : 'Create schedule'}
      saveDisabled={!canSave}
      onDelete={handleDelete}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
