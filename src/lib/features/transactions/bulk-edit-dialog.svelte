<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import * as Select from '$lib/components/ui/select';
  import * as Tabs from '$lib/components/ui/tabs';
  import { Select as SelectPrimitive } from 'bits-ui';
  import DescriptionCombobox from './description-combobox.svelte';
  import { normalizeTagName, tagNameError } from '$lib/features/tags/tag-utils';
  import {
    MAX_PAYEE_LEN,
    normalizePayeeName,
    payeeNameError,
    payeeOptions
  } from '$lib/features/payees/payee-utils';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import {
    bulkEditTransactions,
    type BulkAction,
    type BulkEditResult,
    type BulkTarget,
    type PayeeSummary,
    type TagRef,
    type TagSummary
  } from '$lib/tauri';
  import type { PickerAccount } from '$lib/features/accounts/account-picker-utils';
  import { errMsg } from '$lib/utils/error';
  import { plural } from '$lib/utils/format.svelte';

  let {
    open = $bindable(false),
    kind,
    target,
    count,
    accounts,
    selectionAccountGuids,
    anySource,
    defaultDescription,
    selectionTagList,
    allTags,
    defaultPayee,
    allPayees,
    onDone
  }: {
    open: boolean;
    kind: BulkAction['kind'];
    target: BulkTarget;
    /** Transactions the target covers, for the copy. */
    count: number;
    /** Every account in the book (placeholders included). */
    accounts: PickerAccount[];
    /** Accounts the loaded selected rows post to, most-used first. */
    selectionAccountGuids: string[];
    /** Offer every account as the source, not just the loaded rows' ones
     * ("all matching" reaches rows that were never loaded). */
    anySource: boolean;
    defaultDescription: string;
    /** Tags the loaded selected rows carry, most used first. */
    selectionTagList: TagRef[];
    /** Every tag in the book, most used first. */
    allTags: TagSummary[];
    /** The payee the selected rows share, `''` when they differ. */
    defaultPayee: string;
    /** Every payee in the book, most used first. */
    allPayees: PayeeSummary[];
    onDone: (kind: BulkAction['kind'], result: BulkEditResult) => void;
  } = $props();

  let fromGuid = $state('');
  let toGuid = $state('');
  let description = $state('');
  /** The bar opens tagging as `addTag`; the dialog switches between add and remove. */
  let tagMode = $state<'addTag' | 'removeTag'>('addTag');
  let tagName = $state('');
  let removeGuid = $state('');
  let payeeName = $state('');
  let saving = $state(false);
  let error = $state('');

  const CATEGORY_TYPES = new Set(['EXPENSE', 'INCOME']);

  let byGuid = $derived(new Map(accounts.map((a) => [a.guid, a])));
  let fromOptions = $derived.by(() => {
    const ranked = selectionAccountGuids.flatMap((g) => byGuid.get(g) ?? []);
    if (!anySource) return ranked;
    const seen = new Set(selectionAccountGuids);
    return [...ranked, ...accounts.filter((a) => a.accountType !== 'ROOT' && !seen.has(a.guid))];
  });
  let fromAccount = $derived(byGuid.get(fromGuid) ?? null);
  let isTagging = $derived(kind === 'addTag' || kind === 'removeTag');
  let effectiveKind = $derived<BulkAction['kind']>(isTagging ? tagMode : kind);
  let tagOptions = $derived(
    allTags.map((t) => ({ description: t.name, hint: plural(t.transactionCount, 'transaction') }))
  );
  // "All matching" reaches rows that were never loaded, so offer every tag.
  let removable = $derived<TagRef[]>(anySource ? allTags : selectionTagList);
  let payeeError = $derived(payeeNameError(normalizePayeeName(payeeName)));
  let tagError = $derived(tagName.trim() ? tagNameError(normalizeTagName(tagName)) : null);
  // Amounts move as they are, so only same-commodity accounts can take them.
  let toOptions = $derived(
    accounts.filter(
      (a) =>
        !a.placeholder &&
        a.accountType !== 'ROOT' &&
        a.guid !== fromGuid &&
        (!fromAccount || a.commodity === fromAccount.commodity)
    )
  );

  useDialogReset(
    () => open,
    () => {
      // Recategorizing is the common case: start from the busiest category.
      fromGuid = fromOptions.find((a) => CATEGORY_TYPES.has(a.accountType))?.guid ?? '';
      toGuid = '';
      description = defaultDescription;
      tagMode = kind === 'removeTag' ? 'removeTag' : 'addTag';
      tagName = '';
      removeGuid = removable[0]?.guid ?? '';
      payeeName = defaultPayee;
      saving = false;
      error = '';
    }
  );

  // A new source can rule out the picked target (other commodity, same account).
  $effect(() => {
    if (toGuid && !toOptions.some((a) => a.guid === toGuid)) toGuid = '';
  });

  let action = $derived.by<BulkAction | null>(() => {
    if (effectiveKind === 'addTag') {
      const name = normalizeTagName(tagName);
      return name && !tagError ? { kind: 'addTag', name } : null;
    }
    if (effectiveKind === 'removeTag') {
      return removeGuid ? { kind: 'removeTag', tagGuid: removeGuid } : null;
    }
    if (kind === 'delete') return { kind };
    if (kind === 'setPayee') {
      return payeeError ? null : { kind, name: normalizePayeeName(payeeName) };
    }
    if (kind === 'setDescription') {
      return description.trim() ? { kind, description: description.trim() } : null;
    }
    return fromGuid && toGuid
      ? { kind: 'moveSplits', fromAccountGuid: fromGuid, toAccountGuid: toGuid }
      : null;
  });

  const TITLES: Record<BulkAction['kind'], string> = {
    delete: 'Delete these transactions?',
    moveSplits: 'Move to another account',
    setDescription: 'Rename transactions',
    addTag: 'Tag transactions',
    removeTag: 'Tag transactions',
    setPayee: 'Set the payee'
  };

  const SAVE_LABELS: Record<BulkAction['kind'], string> = {
    delete: 'Delete',
    moveSplits: 'Move',
    setDescription: 'Rename',
    addTag: 'Add tag',
    removeTag: 'Remove tag',
    setPayee: 'Set payee'
  };

  async function apply() {
    if (!action || saving) return;
    saving = true;
    error = '';
    try {
      const result = await bulkEditTransactions(target, action);
      open = false;
      onDone(action.kind, result);
    } catch (e: unknown) {
      error = errMsg(e, 'We couldn’t change these transactions');
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{TITLES[kind]}</Dialog.Title>
      <Dialog.Description>
        {#if kind === 'delete'}
          {plural(count, 'transaction')} will be gone, splits and attachments too. Cleared or reconciled
          ones are left as is.
        {:else if kind === 'moveSplits'}
          Splits of {plural(count, 'transaction')} posted to one account move to another. Amounts stay
          the same; cleared or reconciled splits are left as is.
        {:else if isTagging}
          Add a tag to {plural(count, 'transaction')}, or take one off. Cleared and reconciled ones
          are included: tags don’t touch amounts.
        {:else if kind === 'setPayee'}
          Give {plural(count, 'transaction')} the same payee, or leave it blank to clear it. Cleared and
          reconciled ones are included: payees don’t touch amounts.
        {:else}
          Give {plural(count, 'transaction')} the same description.
        {/if}
        A backup is taken first, so you can restore it from Settings.
      </Dialog.Description>
    </Dialog.Header>

    <form
      class="flex flex-col gap-4"
      onsubmit={(e) => {
        e.preventDefault();
        void apply();
      }}
    >
      {#if kind === 'moveSplits'}
        <div class="flex flex-col gap-1.5">
          <label for="bulk-from" class="text-sm font-medium">From</label>
          <AccountPicker
            id="bulk-from"
            bind:value={fromGuid}
            options={fromOptions}
            placeholder="Pick the account to move from"
          />
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="bulk-to" class="text-sm font-medium">To</label>
          <AccountPicker
            id="bulk-to"
            bind:value={toGuid}
            options={toOptions}
            disabled={!fromGuid}
            placeholder="Pick the account to move to"
            emptyText="No account in the same currency matches."
          />
        </div>
      {:else if isTagging}
        <Tabs.Root bind:value={tagMode}>
          <Tabs.List class="w-full">
            <Tabs.Trigger value="addTag">Add a tag</Tabs.Trigger>
            <Tabs.Trigger value="removeTag" disabled={removable.length === 0}>
              Remove a tag
            </Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
        {#if tagMode === 'addTag'}
          <div class="flex flex-col gap-1.5">
            <label for="bulk-tag" class="text-sm font-medium">Tag</label>
            <DescriptionCombobox
              id="bulk-tag"
              bind:value={tagName}
              options={tagOptions}
              placeholder="Pick one or type a new tag"
              maxlength={60}
            />
            {#if tagError}
              <p role="alert" class="text-xs text-destructive">{tagError}</p>
            {/if}
          </div>
        {:else}
          <div class="flex flex-col gap-1.5">
            <label for="bulk-untag" class="text-sm font-medium">Tag</label>
            <Select.Root
              type="single"
              bind:value={removeGuid}
              items={removable.map((t) => ({ value: t.guid, label: t.name }))}
            >
              <Select.Trigger id="bulk-untag" class="w-full">
                <SelectPrimitive.Value placeholder="Pick a tag" />
              </Select.Trigger>
              <Select.Content>
                {#each removable as t (t.guid)}
                  <Select.Item value={t.guid} label={t.name}>{t.name}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        {/if}
      {:else if kind === 'setPayee'}
        <div class="flex flex-col gap-1.5">
          <label for="bulk-payee" class="text-sm font-medium">Payee</label>
          <DescriptionCombobox
            id="bulk-payee"
            bind:value={payeeName}
            options={payeeOptions(allPayees)}
            placeholder="Pick one or type a new payee"
            maxlength={MAX_PAYEE_LEN + 20}
          />
          {#if payeeError}
            <p role="alert" class="text-xs text-destructive">{payeeError}</p>
          {/if}
        </div>
      {:else if kind === 'setDescription'}
        <div class="flex flex-col gap-1.5">
          <label for="bulk-description" class="text-sm font-medium">Description</label>
          <Input
            id="bulk-description"
            bind:value={description}
            maxlength={500}
            autocomplete="off"
          />
        </div>
      {/if}

      {#if error}
        <p
          role="alert"
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          {error}
        </p>
      {/if}
    </form>

    {#if kind === 'delete'}
      <Dialog.Footer>
        <Button variant="outline" onclick={() => (open = false)} disabled={saving}>Cancel</Button>
        <Button variant="destructive" onclick={apply} disabled={saving}>
          {saving ? 'Deleting…' : `Yes, delete ${plural(count, 'transaction')}`}
        </Button>
      </Dialog.Footer>
    {:else}
      <SaveFooter
        {saving}
        deleting={false}
        confirmDelete={false}
        saveLabel={SAVE_LABELS[effectiveKind]}
        saveDisabled={!action || saving}
        onSave={apply}
        onCancel={() => (open = false)}
      />
    {/if}
  </Dialog.Content>
</Dialog.Root>
