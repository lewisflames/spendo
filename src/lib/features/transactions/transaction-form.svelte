<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import { Info } from '@lucide/svelte';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import TransactionAttachments from '$lib/features/transactions/transaction-attachments.svelte';
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
  import { modKeyLabel } from '$lib/utils/platform';
  import { isIsoDate, today as sharedToday, todayStr } from '$lib/utils/date-utils.svelte';
  import { plural } from '$lib/utils/format.svelte';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  const LAST_ACCOUNTS_KEY = 'tx-last-accounts';
  const RECENT_DESCRIPTIONS_KEY = 'tx-recent-descriptions';

  let {
    mode = 'create',
    guid = null,
    initial = null,
    title,
    headerDescription,
    saveLabel,
    onSaved,
    onSavedAndNew,
    onDeleted,
    onCancel,
    onDuplicate
  }: {
    mode?: 'create' | 'edit';
    guid?: string | null;
    initial?: TransactionDetail | null;
    title: string;
    headerDescription: string;
    /** Overrides the primary action label in create mode (e.g. "Create duplicate"). */
    saveLabel?: string;
    onSaved?: (guid: string) => void;
    onSavedAndNew?: (guid: string) => void;
    onDeleted?: () => void;
    onCancel: () => void;
    onDuplicate?: () => void;
  } = $props();

  let keySeq = $state(0);

  function nextKey(): number {
    keySeq += 1;
    return keySeq;
  }

  function readStoredArray(key: string): string[] {
    try {
      const raw = localStorage.getItem(key);
      if (!raw) return [];
      const parsed: unknown = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === 'string') : [];
    } catch {
      return [];
    }
  }

  function writeStoredArray(key: string, values: string[]): void {
    try {
      localStorage.setItem(key, JSON.stringify(values.slice(0, 20)));
    } catch {
      // Storage full or unavailable — entry speed hints are best-effort.
    }
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
  // Dirty baseline moves after "Save & add another" resets the form.
  let baseline = $state(snapshot);

  let saving = $state(false);
  let savingAndNew = $state(false);
  let deleting = $state(false);
  let deleteDialogOpen = $state(false);
  let confirmUnreconcileOpen = $state(false);
  let discardDialogOpen = $state(false);

  const modKey = modKeyLabel();
  // The save chord belongs to the form, so platforms show the key they
  // actually press instead of a hardcoded Ctrl.
  let saveHint = $derived(
    mode === 'create' && onSavedAndNew
      ? `${modKey}+S saves. Add Shift to jump straight to the next one.`
      : `${modKey}+S saves.`
  );
  let headerText = $derived(headerDescription ? `${headerDescription} ${saveHint}` : saveHint);

  let primaryLabel = $derived(
    saveLabel ?? (mode === 'edit' ? 'Save changes' : 'Create transaction')
  );
  let formError = $state('');
  let showErrors = $state(false);
  let didPrefillAccounts = $state(false);
  let recentDescriptions = $state<string[]>(
    untrack(() => readStoredArray(RECENT_DESCRIPTIONS_KEY))
  );

  const accountsRes = useAsyncData({
    enabled: () => true,
    fetch: fetchAccounts,
    label: 'We couldn’t load your accounts'
  });

  let pickerOptions = $derived(
    flattenAccountsForPicker(accountsRes.data?.trees, { hidePlaceholders: true })
  );
  let knownGuids = $derived(new Set(pickerOptions.map((o) => o.guid)));

  // Batch entry: prefill blank splits with the accounts used last time.
  $effect(() => {
    if (mode !== 'create' || initial || didPrefillAccounts || !accountsRes.data) return;
    const pristine = splits.every((s) => !s.accountGuid && !s.amount.trim());
    if (!pristine) {
      didPrefillAccounts = true;
      return;
    }
    const last = untrack(() => readStoredArray(LAST_ACCOUNTS_KEY)).filter((g) => knownGuids.has(g));
    if (last.length > 0) {
      for (let i = 0; i < splits.length && i < last.length; i += 1) {
        splits[i].accountGuid = last[i];
      }
      baseline = {
        postDate,
        description,
        num,
        notes,
        splits: splits.map((s) => ({ ...s }))
      };
    }
    didPrefillAccounts = true;
  });

  // Shared local date; the guard re-evaluates after midnight (same comparison
  // as `isFutureDate`, but against the reactive clock).
  let today = $derived(sharedToday.value);
  let dateError = $derived.by(() => {
    if (!postDate) return 'Pick a date for this transaction';
    if (!isIsoDate(postDate)) return 'Pick a valid date';
    if (postDate > today)
      return 'This date is in the future. Balances only include today or earlier';
    return '';
  });

  let errors = $derived(splitErrors(splits));
  let balance = $derived(splitBalance(splits));
  let balanced = $derived(isBalanced(balance));

  // Saving re-creates every split as 'n', so edits and deletes of cleared or
  // reconciled transactions need the user's confirmation first.
  let lockedSplitCount = $derived(
    initial?.splits.filter((s) => s.reconcileState === 'c' || s.reconcileState === 'y').length ?? 0
  );
  let hasLockedSplits = $derived(lockedSplitCount > 0);

  // Receipts are deleted permanently with their transaction.
  let attachmentCount = $derived(initial?.attachmentCount ?? 0);

  let dirty = $derived.by(() => {
    if (
      postDate !== baseline.postDate ||
      description !== baseline.description ||
      num !== baseline.num ||
      notes !== baseline.notes ||
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
  // buttons stay honestly disabled until the form can actually save.
  let effectiveShowErrors = $derived(showErrors || dirty);

  let issues = $derived.by(() => {
    const list: string[] = [];
    if (dateError) list.push(dateError);
    if (splits.length < 2) list.push('Add at least two splits so both sides balance');
    for (const e of errors) {
      if (e) {
        list.push(e);
        if (list.length >= 4) break;
      }
    }
    if (list.length === 0 && balance !== null && !balanced) {
      list.push('Splits need to net to zero. Debits should equal credits');
    }
    return list;
  });

  let canSave = $derived(
    !saving &&
      !savingAndNew &&
      !deleting &&
      !dateError &&
      splits.length >= 2 &&
      errors.every((e) => !e) &&
      balanced
  );

  function rememberForNextTime(accountGuids: string[], desc: string) {
    const valid = accountGuids.filter((g) => knownGuids.has(g));
    if (valid.length > 0) writeStoredArray(LAST_ACCOUNTS_KEY, valid);
    const trimmed = desc.trim();
    if (trimmed) {
      const next = [trimmed, ...recentDescriptions.filter((d) => d !== trimmed)].slice(0, 20);
      recentDescriptions = next;
      writeStoredArray(RECENT_DESCRIPTIONS_KEY, next);
    }
  }

  function buildInput(): TxInput {
    return {
      postDate,
      description: description.trim() ? description.trim() : null,
      num: num.trim() ? num.trim() : null,
      notes: notes.trim() ? notes.trim() : null,
      splits: splits.map(splitToInput)
    };
  }

  function focusDescription() {
    void tick().then(() => {
      document.getElementById('tx-desc')?.focus();
    });
  }

  function focusFormError() {
    void tick().then(() => {
      document.getElementById('tx-form-error')?.focus();
    });
  }

  async function handleSave() {
    if (saving || savingAndNew || deleting) return;
    if (!canSave) {
      showErrors = true;
      return;
    }
    // Saving a cleared or reconciled transaction re-creates its splits as 'n';
    // confirm that consequence before writing anything.
    if (mode === 'edit' && hasLockedSplits) {
      confirmUnreconcileOpen = true;
      return;
    }
    await saveNow(false);
  }

  async function saveNow(unreconcile: boolean) {
    if (saving || savingAndNew || deleting) return;
    saving = true;
    formError = '';
    try {
      const input = { ...buildInput(), unreconcile };
      if (mode === 'edit' && guid) {
        await updateTransaction(guid, input);
        rememberForNextTime(
          splits.map((s) => s.accountGuid),
          description
        );
        onSaved?.(guid);
      } else {
        const newGuid = await createTransaction(input);
        rememberForNextTime(
          splits.map((s) => s.accountGuid),
          description
        );
        onSaved?.(newGuid);
      }
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t save this transaction. Please try again');
      focusFormError();
    } finally {
      saving = false;
    }
  }

  async function handleSaveAndNew() {
    if (mode !== 'create' || saving || savingAndNew || deleting) return;
    if (!canSave) {
      showErrors = true;
      return;
    }
    savingAndNew = true;
    formError = '';
    try {
      const newGuid = await createTransaction(buildInput());
      rememberForNextTime(
        splits.map((s) => s.accountGuid),
        description
      );
      onSavedAndNew?.(newGuid);
      // Keep the date and accounts for fast batch entry; clear the rest.
      description = '';
      num = '';
      notes = '';
      splits = splits.map((s) => ({
        ...makeSplitDraft(nextKey()),
        accountGuid: knownGuids.has(s.accountGuid) ? s.accountGuid : '',
        showQuantity: s.showQuantity
      }));
      while (splits.length < 2) splits.push(makeSplitDraft(nextKey()));
      baseline = {
        postDate,
        description,
        num,
        notes,
        splits: splits.map((s) => ({ ...s }))
      };
      showErrors = false;
      focusDescription();
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t save this transaction. Please try again');
      focusFormError();
    } finally {
      savingAndNew = false;
    }
  }

  function requestCancel() {
    if (saving || savingAndNew || deleting) return;
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
      await deleteTransaction(guid, hasLockedSplits);
      deleteDialogOpen = false;
      onDeleted?.();
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t delete this transaction. Please try again');
      deleteDialogOpen = false;
      focusFormError();
    } finally {
      deleting = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || e.key.toLowerCase() !== 's') return;
    // Don’t hijack the browser save dialog when a native dialog has focus,
    // and never save behind the delete/unreconcile/discard confirmations.
    if (e.defaultPrevented || deleteDialogOpen || confirmUnreconcileOpen || discardDialogOpen)
      return;
    e.preventDefault();
    if (e.shiftKey && mode === 'create' && onSavedAndNew) {
      void handleSaveAndNew();
    } else {
      void handleSave();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="flex w-full flex-col gap-6">
  <PageHeader {title} description={headerText}>
    {#if mode === 'edit'}
      <Button
        type="button"
        variant="outline"
        onclick={() => (deleteDialogOpen = true)}
        disabled={saving || savingAndNew || deleting}
        title="Delete transaction"
      >
        Delete
      </Button>
      {#if onDuplicate}
        <Button
          type="button"
          variant="outline"
          onclick={onDuplicate}
          disabled={saving || savingAndNew || deleting}
          title="Duplicate transaction"
        >
          Duplicate
        </Button>
      {/if}
    {/if}
    <Button
      type="button"
      variant="outline"
      onclick={requestCancel}
      disabled={saving || savingAndNew || deleting}
    >
      Cancel
    </Button>
    {#if mode === 'create' && onSavedAndNew}
      <Button
        type="button"
        variant="outline"
        onclick={() => void handleSaveAndNew()}
        disabled={!canSave || saving || savingAndNew || deleting}
        title={`Save & add another (${modKey}+Shift+S)`}
      >
        {savingAndNew ? 'Saving…' : 'Save & add another'}
      </Button>
    {/if}
    <Button
      type="button"
      onclick={() => void handleSave()}
      disabled={saving || savingAndNew || deleting || !canSave}
    >
      {saving ? 'Saving…' : primaryLabel}
    </Button>
  </PageHeader>

  <Card.Root>
    <IconCardHeader
      title="Details"
      description="How this shows up in lists and reports: a short description, date and reference."
    >
      {#snippet icon()}
        <Info aria-hidden="true" class="size-4 text-muted-foreground" />
      {/snippet}
    </IconCardHeader>
    <Card.Content class="flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <label for="tx-desc" class="text-sm font-medium">Description</label>
        <Input
          id="tx-desc"
          bind:value={description}
          list="tx-desc-history"
          placeholder="What was this for?"
          maxlength={500}
          autofocus
          autocomplete="off"
        />
        <datalist id="tx-desc-history">
          {#each recentDescriptions as recent (recent)}
            <option value={recent}></option>
          {/each}
        </datalist>
      </div>
      <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <div class="flex flex-col gap-1.5">
          <label for="tx-date" class="text-sm font-medium">Date</label>
          <DatePicker
            id="tx-date"
            bind:value={postDate}
            placeholder="Pick a date"
            invalid={!!dateError}
            describedBy={dateError ? 'tx-date-error' : 'tx-date-hint'}
          />
          {#if dateError}
            <p id="tx-date-error" role="alert" class="text-xs text-destructive">{dateError}</p>
          {:else}
            <p id="tx-date-hint" class="text-xs text-muted-foreground">
              Dates after today stay out of balances, budgets and net worth.
            </p>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="tx-num" class="text-sm font-medium">Number (optional)</label>
          <Input id="tx-num" bind:value={num} placeholder="e.g. Check #101" maxlength={100} />
          <p class="text-xs text-muted-foreground">
            A check or reference number, if you’ve got one.
          </p>
        </div>
      </div>
      <div class="flex flex-col gap-1.5">
        <label for="tx-notes" class="text-sm font-medium">Notes (optional)</label>
        <Textarea
          id="tx-notes"
          bind:value={notes}
          placeholder="Anything worth remembering?"
          rows={3}
          maxlength={2000}
        />
      </div>
    </Card.Content>
  </Card.Root>

  <SplitEditor
    bind:splits
    options={pickerOptions}
    loading={accountsRes.loading}
    loadError={accountsRes.error ?? ''}
    showErrors={effectiveShowErrors}
    makeKey={nextKey}
  />

  {#if mode === 'edit' && hasLockedSplits}
    <p
      role="note"
      class="rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-300"
    >
      {plural(lockedSplitCount, 'split')} here {lockedSplitCount === 1 ? 'is' : 'are'} cleared or reconciled.
      Saving puts them back to New; you’ll be asked to confirm.
    </p>
  {/if}

  {#if !canSave && (dirty || showErrors)}
    <div
      role="alert"
      class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
    >
      <p class="font-medium">Sort out {plural(issues.length, 'thing')} before saving:</p>
      <ul class="mt-1 list-inside list-disc">
        {#each issues as issue (issue)}
          <li>{issue}</li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if formError}
    <p id="tx-form-error" role="alert" tabindex="-1" class="text-sm text-destructive outline-none">
      {formError}
    </p>
  {/if}

  <TransactionAttachments txGuid={mode === 'edit' ? guid : null} />

  <Dialog.Root bind:open={confirmUnreconcileOpen}>
    <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
      <Dialog.Header>
        <Dialog.Title>Save and unreconcile?</Dialog.Title>
        <Dialog.Description>
          Saving re-creates this transaction’s splits, so every cleared or reconciled split goes
          back to New. Reconciled splits also drop out of the account’s starting balance for its
          next reconciliation, and all of them show up again as candidates.
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button
          variant="outline"
          onclick={() => (confirmUnreconcileOpen = false)}
          disabled={saving}
        >
          Cancel
        </Button>
        <Button
          onclick={() => {
            confirmUnreconcileOpen = false;
            void saveNow(true);
          }}
          disabled={saving}
        >
          {saving ? 'Saving…' : 'Save anyway'}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  <Dialog.Root bind:open={deleteDialogOpen}>
    <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
      <Dialog.Header>
        <Dialog.Title>Delete this one?</Dialog.Title>
        <Dialog.Description>
          It’ll be gone for good, splits and all.
          {#if attachmentCount > 0}
            Its {plural(attachmentCount, 'attachment')} are deleted permanently too.
          {/if}
        </Dialog.Description>
      </Dialog.Header>
      {#if hasLockedSplits}
        <p
          role="note"
          class="rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-300"
        >
          {plural(lockedSplitCount, 'split')} here {lockedSplitCount === 1 ? 'is' : 'are'} cleared or
          reconciled. Deleting removes them from the account’s reconciliation history.
        </p>
      {/if}
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
          You’ve got unsaved edits. Leave now and you’ll lose them.
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
