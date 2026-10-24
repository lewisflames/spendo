<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import {
    bulkEditTransactions,
    type BulkAction,
    type BulkEditResult,
    type BulkTarget
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
    onDone: (kind: BulkAction['kind'], result: BulkEditResult) => void;
  } = $props();

  let fromGuid = $state('');
  let toGuid = $state('');
  let description = $state('');
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
      saving = false;
      error = '';
    }
  );

  // A new source can rule out the picked target (other commodity, same account).
  $effect(() => {
    if (toGuid && !toOptions.some((a) => a.guid === toGuid)) toGuid = '';
  });

  let action = $derived.by<BulkAction | null>(() => {
    if (kind === 'delete') return { kind };
    if (kind === 'setDescription') {
      return description.trim() ? { kind, description: description.trim() } : null;
    }
    return fromGuid && toGuid ? { kind, fromAccountGuid: fromGuid, toAccountGuid: toGuid } : null;
  });

  const TITLES: Record<BulkAction['kind'], string> = {
    delete: 'Delete these transactions?',
    moveSplits: 'Move to another account',
    setDescription: 'Rename transactions'
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
        saveLabel={kind === 'moveSplits' ? 'Move' : 'Rename'}
        saveDisabled={!action || saving}
        onSave={apply}
        onCancel={() => (open = false)}
      />
    {/if}
  </Dialog.Content>
</Dialog.Root>
