<script lang="ts">
  import { untrack } from 'svelte';
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
    createTransaction,
    deleteTransaction,
    fetchAccounts,
    updateTransaction,
    type TransactionDetail,
    type TxInput
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { isFutureDate, isIsoDate, todayStr } from '$lib/utils/date-utils.svelte';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

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
    initial?: TransactionDetail | null;
    onSaved?: (guid: string) => void;
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
  // initial prop is captured once; {#key} remounts when the transaction changes.
  const snapshot = untrack(() => {
    if (!initial) {
      return {
        postDate: todayStr(),
        description: '',
        num: '',
        notes: '',
        splits: [makeSplitDraft(nextKey()), makeSplitDraft(nextKey())]
      };
    }
    const drafts = initial.splits.map((s) => draftFromSplit(s, nextKey()));
    while (drafts.length < 2) drafts.push(makeSplitDraft(nextKey()));
    return {
      postDate: initial.postDate.slice(0, 10),
      description: initial.description ?? '',
      num: initial.num ?? '',
      notes: initial.notes ?? '',
      splits: drafts
    };
  });

  let postDate = $state(snapshot.postDate);
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

  let dateError = $derived.by(() => {
    if (!postDate) return 'Date is required';
    if (!isIsoDate(postDate)) return 'Use a valid date (YYYY-MM-DD)';
    if (isFutureDate(postDate)) return 'Future dates are not allowed';
    return '';
  });

  let errors = $derived(splitErrors(splits));
  let balance = $derived(splitBalance(splits));
  let balanced = $derived(isBalanced(balance));

  let canSave = $derived(
    !saving && !deleting && !dateError && splits.length >= 2 && errors.every((e) => !e) && balanced
  );

  function buildInput(): TxInput {
    return {
      postDate,
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
        await updateTransaction(guid, input);
        onSaved?.(guid);
      } else {
        const newGuid = await createTransaction(input);
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
      onDeleted?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to delete transaction');
      formError = msg;
      onError?.(msg);
    } finally {
      deleting = false;
    }
  }
</script>

<div class="flex w-full flex-col gap-6">
  <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
    <div class="order-2 min-w-0 lg:order-1">
      <SplitEditor
        bind:splits
        options={pickerOptions}
        loading={accountsRes.loading}
        loadError={accountsRes.error ?? ''}
        {showErrors}
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
          <div class="flex flex-col gap-1.5">
            <label for="tx-notes" class="text-sm font-medium">Notes (optional)</label>
            <Textarea
              id="tx-notes"
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
      Delete this transaction? Its splits will be removed. This can be undone from history
      (newest-first).
    </p>
  {/if}

  <SaveFooter
    layout="form"
    {mode}
    {saving}
    {deleting}
    {confirmDelete}
    saveLabel={mode === 'edit' ? 'Save changes' : 'Create transaction'}
    saveDisabled={saving || deleting}
    onDelete={handleDelete}
    onSave={handleSave}
    {onCancel}
  />
</div>
