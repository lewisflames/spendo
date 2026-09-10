<script lang="ts">
  import { untrack } from 'svelte';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import * as Card from '$lib/components/ui/card';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import {
    SplitEditor,
    draftFromSplit,
    isBalanced,
    makeSplitDraft,
    splitBalance,
    splitErrors,
    splitToInput,
    type SplitDraft
  } from '$lib/components/shared/split-editor';
  import {
    createSchedule,
    deleteSchedule,
    fetchAccounts,
    updateSchedule,
    type Schedule,
    type ScheduleFrequency,
    type ScheduleInput
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { FREQUENCY_OPTIONS } from './schedule-utils';

  let {
    mode = 'create',
    guid = null,
    initial = null,
    onSaved,
    onDeleted,
    onCancel,
    onError
  }: {
    mode?: 'create' | 'edit';
    guid?: string | null;
    initial?: Schedule | null;
    onSaved?: (schedule: Schedule) => void;
    onDeleted?: () => void;
    onCancel: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let keySeq = $state(0);

  function nextKey(): number {
    keySeq += 1;
    return keySeq;
  }

  // The edit page mounts this form only after the payload resolves, so the
  // initial prop is captured once; {#key} remounts when the schedule changes.
  const snapshot = untrack(() => {
    const drafts = initial
      ? initial.splits.map((s) => draftFromSplit(s, nextKey()))
      : [makeSplitDraft(nextKey()), makeSplitDraft(nextKey())];
    while (drafts.length < 2) drafts.push(makeSplitDraft(nextKey()));
    return {
      name: initial?.name ?? '',
      frequency: initial?.frequency ?? 'monthly',
      intervalMult: initial ? String(initial.intervalMult) : '1',
      startDate: initial ? initial.startDate : todayStr(),
      endDate: initial?.endDate ?? '',
      maxOccurrences: initial?.maxOccurrences != null ? String(initial.maxOccurrences) : '',
      description: initial?.description ?? '',
      num: initial?.num ?? '',
      notes: initial?.notes ?? '',
      splits: drafts
    };
  });

  let name = $state(snapshot.name);
  let frequency = $state<ScheduleFrequency>(snapshot.frequency);
  let intervalMult = $state(snapshot.intervalMult);
  let startDate = $state(snapshot.startDate);
  let endDate = $state(snapshot.endDate);
  let maxOccurrences = $state(snapshot.maxOccurrences);
  let description = $state(snapshot.description);
  let num = $state(snapshot.num);
  let notes = $state(snapshot.notes);
  let splits = $state<SplitDraft[]>(snapshot.splits);

  let saving = $state(false);
  let deleting = $state(false);
  let confirmDelete = $state(false);
  let formError = $state('');
  let showErrors = $state(false);

  const accountsRes = useAsyncData({
    enabled: () => true,
    fetch: fetchAccounts,
    label: 'Failed to load accounts'
  });

  let pickerOptions = $derived(
    flattenAccountsForPicker(accountsRes.data?.trees, { hidePlaceholders: true })
  );

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

  let errors = $derived(splitErrors(splits));
  let balance = $derived(splitBalance(splits));
  let balanced = $derived(isBalanced(balance));

  let canSave = $derived(
    !saving &&
      !deleting &&
      !nameError &&
      !multError &&
      !startError &&
      !endError &&
      !countError &&
      splits.length >= 2 &&
      errors.every((e) => !e) &&
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
      splits: splits.map(splitToInput)
    };
  }

  async function handleSave() {
    if (saving || deleting) return;
    if (!canSave) {
      showErrors = true;
      return;
    }
    saving = true;
    formError = '';
    try {
      const input = buildInput();
      if (mode === 'edit' && guid) {
        onSaved?.(await updateSchedule(guid, input));
      } else {
        onSaved?.(await createSchedule(input));
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
      onDeleted?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to delete schedule');
      formError = msg;
      onError?.(msg);
    } finally {
      deleting = false;
    }
  }
</script>

<div class="flex w-full flex-col gap-6">
  <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
    <div class="order-2 flex min-w-0 flex-col gap-6 lg:order-1">
      <Card.Root>
        <Card.Header>
          <Card.Title>Recurrence</Card.Title>
          <Card.Description>
            Nothing posts automatically — each due occurrence surfaces as a reminder to post or
            skip.
          </Card.Description>
        </Card.Header>
        <Card.Content class="flex flex-col gap-4">
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
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
                  type="text"
                  inputmode="numeric"
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
                  type="text"
                  inputmode="numeric"
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
                : `${initial.occurrencesPosted} times`} — changing the start date won't move the next
              due date.
            </p>
          {/if}
        </Card.Content>
      </Card.Root>

      <SplitEditor
        bind:splits
        options={pickerOptions}
        loading={accountsRes.loading}
        loadError={accountsRes.error ?? ''}
        {showErrors}
        title="Template splits"
        idPrefix="sched-split"
        makeKey={nextKey}
      />
    </div>

    <div class="order-1 lg:order-2">
      <Card.Root>
        <Card.Header>
          <Card.Title>Details</Card.Title>
        </Card.Header>
        <Card.Content class="flex flex-col gap-4">
          <div class="flex flex-col gap-1.5">
            <label for="sched-name" class="text-sm font-medium">Name</label>
            <Input
              id="sched-name"
              bind:value={name}
              placeholder="e.g. Monthly rent"
              maxlength={200}
              autofocus
              aria-invalid={showErrors && !!nameError}
            />
            {#if showErrors && nameError}
              <p role="alert" class="text-xs text-destructive">{nameError}</p>
            {/if}
          </div>
          <div class="flex flex-col gap-1.5">
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
          <div class="flex flex-col gap-1.5">
            <label for="sched-notes" class="text-sm font-medium">Notes (optional)</label>
            <Textarea
              id="sched-notes"
              bind:value={notes}
              placeholder="Optional notes"
              rows={3}
              maxlength={2000}
            />
          </div>
        </Card.Content>
      </Card.Root>
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
      Delete this schedule? Future reminders stop; already-posted transactions stay in the ledger.
    </p>
  {/if}

  <SaveFooter
    layout="form"
    {mode}
    {saving}
    {deleting}
    {confirmDelete}
    saveLabel={mode === 'edit' ? 'Save changes' : 'Create schedule'}
    saveDisabled={saving || deleting}
    onDelete={handleDelete}
    onSave={handleSave}
    {onCancel}
  />
</div>
