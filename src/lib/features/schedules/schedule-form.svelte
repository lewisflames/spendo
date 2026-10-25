<script lang="ts">
  import { tick, untrack } from 'svelte';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';
  import { Switch } from '$lib/components/ui/switch';
  import * as Card from '$lib/components/ui/card';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import { Info, Repeat, Trash2 } from '@lucide/svelte';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import PageHeader, { type Breadcrumb } from '$lib/components/layout/page-header.svelte';
  import OverflowMenu from '$lib/components/shared/overflow-menu.svelte';
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
    fetchPayees,
    updateSchedule,
    type Schedule,
    type ScheduleFrequency,
    type ScheduleInput
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { modKeyLabel } from '$lib/utils/platform';
  import { isIsoDate, todayStr } from '$lib/utils/date-utils.svelte';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { FREQUENCY_OPTIONS } from './schedule-utils';
  import DescriptionCombobox from '$lib/features/transactions/description-combobox.svelte';
  import {
    MAX_PAYEE_LEN,
    normalizePayeeName,
    payeeNameError,
    payeeOptions
  } from '$lib/features/payees/payee-utils';

  let {
    mode = 'create',
    guid = null,
    initial = null,
    title,
    breadcrumbs = [],
    headerDescription,
    onSaved,
    onDeleted,
    onCancel
  }: {
    mode?: 'create' | 'edit';
    guid?: string | null;
    initial?: Schedule | null;
    title: string;
    breadcrumbs?: Breadcrumb[];
    headerDescription: string;
    onSaved?: (schedule: Schedule) => void;
    onDeleted?: () => void;
    onCancel: () => void;
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
      payee: initial?.payee ?? '',
      autoPost: initial?.autoPost ?? false,
      spreadTotal: initial?.spreadTotal ?? false,
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
  let payee = $state(snapshot.payee);
  let autoPost = $state(snapshot.autoPost);
  let spreadTotal = $state(snapshot.spreadTotal);
  let splits = $state<SplitDraft[]>(snapshot.splits);

  let saving = $state(false);
  let deleting = $state(false);
  let deleteDialogOpen = $state(false);
  let discardDialogOpen = $state(false);
  let formError = $state('');
  let showErrors = $state(false);

  const modKey = modKeyLabel();
  let headerText = $derived(`${headerDescription} ${modKey}+S saves.`);

  let intervalUnit = $derived.by(() => {
    const one = Math.abs(Number(intervalMult)) === 1;
    switch (frequency) {
      case 'daily':
        return one ? 'day' : 'days';
      case 'weekly':
        return one ? 'week' : 'weeks';
      case 'biweekly':
        return '× 2 weeks';
      case 'monthly':
        return one ? 'month' : 'months';
      case 'quarterly':
        return one ? 'quarter' : 'quarters';
      default:
        return one ? 'year' : 'years';
    }
  });
  let occurrenceUnit = $derived(
    Math.abs(Number(maxOccurrences)) === 1 ? 'occurrence' : 'occurrences'
  );
  // Dirty baseline is the initial snapshot; {#key} remounts on schedule change.
  let baseline = $state(snapshot);

  const accountsRes = useAsyncData({
    enabled: () => true,
    fetch: fetchAccounts,
    label: 'We couldn’t load your accounts'
  });

  let pickerOptions = $derived(
    flattenAccountsForPicker(accountsRes.data?.trees, { hidePlaceholders: true })
  );

  const payeesRes = useAsyncData({
    enabled: () => true,
    fetch: fetchPayees,
    label: 'We couldn’t load your payees'
  });
  let payeeSuggestions = $derived(payeeOptions(payeesRes.data ?? []));
  let payeeError = $derived(payeeNameError(normalizePayeeName(payee)));

  let nameError = $derived.by(() => {
    if (!name.trim()) return 'Give this schedule a name';
    if (name.trim().length > 200) return 'Keep the name under 200 characters';
    return '';
  });

  let multError = $derived.by(() => {
    const n = Number(intervalMult);
    if (!Number.isInteger(n) || n < 1 || n > 100) return 'Use a whole number between 1 and 100';
    return '';
  });

  let startError = $derived.by(() => {
    if (!startDate) return 'Pick a start date';
    if (!isIsoDate(startDate)) return 'Pick a valid date';
    return '';
  });

  let endError = $derived.by(() => {
    if (!endDate) return '';
    if (!isIsoDate(endDate)) return 'Pick a valid date';
    if (!startError && endDate < startDate) return 'The end date is before the start date';
    return '';
  });

  let countError = $derived.by(() => {
    if (!maxOccurrences.trim()) return '';
    const n = Number(maxOccurrences);
    if (!Number.isInteger(n) || n < 1 || n > 1200) return 'Use a whole number between 1 and 1200';
    return '';
  });

  let spreadError = $derived.by(() => {
    if (!spreadTotal) return '';
    if (!maxOccurrences.trim()) return 'Set how many times it repeats to spread the total';
    if (splits.some((s) => s.showQuantity && s.quantity.trim()))
      return 'A spread total can’t carry share quantities';
    return '';
  });

  let errors = $derived(splitErrors(splits));
  let balance = $derived(splitBalance(splits));
  let balanced = $derived(isBalanced(balance));

  let canSave = $derived(
    !saving &&
      !deleting &&
      !nameError &&
      !payeeError &&
      !multError &&
      !startError &&
      !endError &&
      !countError &&
      !spreadError &&
      splits.length >= 2 &&
      errors.every((e) => !e) &&
      balanced
  );

  let dirty = $derived.by(() => {
    if (
      name !== baseline.name ||
      frequency !== baseline.frequency ||
      intervalMult !== baseline.intervalMult ||
      startDate !== baseline.startDate ||
      endDate !== baseline.endDate ||
      maxOccurrences !== baseline.maxOccurrences ||
      description !== baseline.description ||
      num !== baseline.num ||
      payee !== baseline.payee ||
      autoPost !== baseline.autoPost ||
      spreadTotal !== baseline.spreadTotal ||
      splits.length !== baseline.splits.length
    ) {
      return true;
    }
    return splits.some((s, i) => {
      const b = baseline.splits[i];
      return (
        s.accountGuid !== b.accountGuid ||
        s.amount !== b.amount ||
        s.quantity !== b.quantity ||
        s.showQuantity !== b.showQuantity ||
        s.memo !== b.memo
      );
    });
  });

  // Show inline split errors live once the user has touched the form; the save
  // button stays honestly disabled until the form can actually save.
  let effectiveShowErrors = $derived(showErrors || dirty);

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
      payee: normalizePayeeName(payee) || null,
      splits: splits.map(splitToInput),
      autoPost,
      spreadTotal
    };
  }

  function focusFormError() {
    void tick().then(() => {
      document.getElementById('sched-form-error')?.focus();
    });
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
      formError = errMsg(e, 'We couldn’t save this schedule');
      focusFormError();
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || e.key.toLowerCase() !== 's') return;
    // Never save behind the delete/discard confirmations.
    if (e.defaultPrevented || deleteDialogOpen || discardDialogOpen) return;
    e.preventDefault();
    void handleSave();
  }

  function requestCancel() {
    if (saving || deleting) return;
    if (!dirty) {
      onCancel();
      return;
    }
    discardDialogOpen = true;
  }

  async function handleDeleteConfirm() {
    if (mode !== 'edit' || !guid || deleting) return;
    deleting = true;
    formError = '';
    try {
      await deleteSchedule(guid);
      deleteDialogOpen = false;
      onDeleted?.();
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t delete this schedule');
      deleteDialogOpen = false;
      focusFormError();
    } finally {
      deleting = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="page-stack w-full">
  <PageHeader {title} description={headerText} {breadcrumbs}>
    <Button type="button" variant="outline" onclick={requestCancel} disabled={saving || deleting}>
      Cancel
    </Button>
    <Button
      type="button"
      onclick={() => void handleSave()}
      disabled={saving || deleting || !canSave}
    >
      {saving ? 'Saving…' : mode === 'edit' ? 'Save changes' : 'Create schedule'}
    </Button>
    {#if mode === 'edit'}
      <OverflowMenu
        actions={[
          {
            label: 'Delete',
            icon: Trash2,
            destructive: true,
            disabled: saving || deleting,
            onSelect: () => (deleteDialogOpen = true)
          }
        ]}
      />
    {/if}
  </PageHeader>

  <Card.Root>
    <IconCardHeader
      title="Details"
      description="What this is for: the name and details used when it posts."
    >
      {#snippet icon()}
        <Info aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <label for="sched-name" class="text-sm font-medium">Name</label>
        <Input
          id="sched-name"
          bind:value={name}
          placeholder="e.g. Monthly rent"
          maxlength={200}
          autofocus
          aria-invalid={effectiveShowErrors && !!nameError}
        />
        {#if effectiveShowErrors && nameError}
          <p role="alert" class="text-xs text-destructive">{nameError}</p>
        {/if}
      </div>
      <div class="grid grid-cols-1 gap-4 @xl:grid-cols-2">
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
      </div>
      <div class="flex flex-col gap-1.5">
        <label for="sched-payee" class="text-sm font-medium">Payee (optional)</label>
        <DescriptionCombobox
          id="sched-payee"
          bind:value={payee}
          options={payeeSuggestions}
          placeholder="e.g. your landlord"
          maxlength={MAX_PAYEE_LEN + 20}
        />
        {#if payeeError}
          <p role="alert" class="text-xs text-destructive">{payeeError}</p>
        {/if}
      </div>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <IconCardHeader
      title="Recurrence"
      description={autoPost
        ? 'Due occurrences post on their own when you open the book.'
        : 'Nothing posts on its own. When one is due you’ll get a reminder to post or skip it.'}
    >
      {#snippet icon()}
        <Repeat aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="grid grid-cols-1 gap-4 @xl:grid-cols-2 @3xl:grid-cols-3">
        <div class="flex flex-col gap-1.5">
          <label for="sched-freq" class="text-sm font-medium">Repeats</label>
          <Select.Root
            type="single"
            value={frequency}
            items={FREQUENCY_OPTIONS}
            onValueChange={(v) => {
              if (FREQUENCY_OPTIONS.some((o) => o.value === v)) frequency = v as ScheduleFrequency;
            }}
          >
            <Select.Trigger id="sched-freq" class="w-full">
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
            <span class="text-sm text-muted-foreground">{intervalUnit}</span>
          </div>
          {#if multError}
            <p role="alert" class="text-xs text-destructive">{multError}</p>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="sched-start" class="text-sm font-medium">First due date</label>
          <DatePicker
            id="sched-start"
            bind:value={startDate}
            placeholder="Pick a date"
            invalid={!!startError}
            describedBy={startError ? 'sched-start-error' : undefined}
          />
          {#if startError}
            <p id="sched-start-error" role="alert" class="text-xs text-destructive">
              {startError}
            </p>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="sched-end" class="text-sm font-medium">End date (optional)</label>
          <DatePicker
            id="sched-end"
            bind:value={endDate}
            placeholder="No end date"
            invalid={!!endError}
            describedBy={endError ? 'sched-end-error' : undefined}
            clearable
          />
          {#if endError}
            <p id="sched-end-error" role="alert" class="text-xs text-destructive">{endError}</p>
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
            <span class="text-sm text-muted-foreground">{occurrenceUnit}</span>
          </div>
          {#if countError}
            <p role="alert" class="text-xs text-destructive">{countError}</p>
          {/if}
        </div>
      </div>
      <div class="flex flex-col gap-3">
        <div class="flex items-start justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <label for="sched-auto" class="text-sm font-medium">Post automatically</label>
            <p class="text-xs text-muted-foreground">
              Catch up on what’s due each time you open the book, instead of reminding you
            </p>
          </div>
          <Switch id="sched-auto" bind:checked={autoPost} />
        </div>
        <div class="flex items-start justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <label for="sched-spread" class="text-sm font-medium">Spread a total</label>
            <p class="text-xs text-muted-foreground">
              Treat the split amounts as a total and post an equal share each time, e.g. to
              depreciate a device. The shares add up exactly
            </p>
          </div>
          <Switch id="sched-spread" bind:checked={spreadTotal} />
        </div>
        {#if spreadError}
          <p role="alert" class="text-xs text-destructive">{spreadError}</p>
        {/if}
      </div>
      {#if mode === 'edit' && initial && initial.occurrencesPosted + initial.skippedCount > 0}
        <p class="text-xs text-muted-foreground">
          {initial.occurrencesPosted > 0
            ? `Already posted ${initial.occurrencesPosted === 1 ? 'once' : `${initial.occurrencesPosted} times`}.`
            : 'No occurrences have been posted yet.'}
          {#if initial.skippedCount > 0}
            {initial.skippedCount}
            {initial.skippedCount === 1 ? 'occurrence was skipped' : 'occurrences were skipped'}.
          {/if}
          Changing the start date won’t move what’s due next.
        </p>
      {/if}
    </Card.Content>
  </Card.Root>

  <SplitEditor
    bind:splits
    options={pickerOptions}
    loading={accountsRes.loading}
    loadError={accountsRes.error ?? ''}
    showErrors={effectiveShowErrors}
    title="Template splits"
    idPrefix="sched-split"
    makeKey={nextKey}
  />

  {#if formError}
    <p
      id="sched-form-error"
      role="alert"
      tabindex="-1"
      class="text-sm text-destructive outline-none"
    >
      {formError}
    </p>
  {/if}

  <Dialog.Root bind:open={deleteDialogOpen}>
    <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
      <Dialog.Header>
        <Dialog.Title>Delete this schedule?</Dialog.Title>
        <Dialog.Description>
          You won’t be reminded again. Anything already posted stays in your ledger.
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button variant="outline" onclick={() => (deleteDialogOpen = false)} disabled={deleting}>
          Cancel
        </Button>
        <Button variant="destructive" onclick={handleDeleteConfirm} disabled={deleting}>
          {deleting ? 'Deleting…' : 'Yes, delete'}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  <Dialog.Root bind:open={discardDialogOpen}>
    <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
      <Dialog.Header>
        <Dialog.Title>Lose your changes?</Dialog.Title>
        <Dialog.Description>
          You’ve got unsaved edits here. Leaving now means starting over.
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button variant="outline" onclick={() => (discardDialogOpen = false)}>Keep editing</Button>
        <Button
          variant="destructive"
          onclick={() => {
            discardDialogOpen = false;
            onCancel();
          }}
        >
          Discard changes
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
</div>
