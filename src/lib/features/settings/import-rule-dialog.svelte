<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import {
    createImportRule,
    deleteImportRule,
    updateImportRule,
    type ImportRule,
    type ImportRuleInput
  } from '$lib/tauri';
  import type { PickerAccount } from '$lib/features/accounts/account-picker-utils';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  let {
    open = $bindable(false),
    options,
    rule = null,
    nextPriority,
    onSaved
  }: {
    open: boolean;
    options: PickerAccount[];
    rule?: ImportRule | null;
    nextPriority: number;
    onSaved: () => void;
  } = $props();

  let pattern = $state('');
  let matchType = $state<ImportRuleInput['matchType']>('contains');
  let accountGuid = $state('');
  let priority = $state(1);
  let saving = $state(false);
  let confirmDelete = $state(false);
  let deleting = $state(false);

  const matchItems: { value: ImportRuleInput['matchType']; label: string }[] = [
    { value: 'contains', label: 'Contains' },
    { value: 'equals', label: 'Exact match' }
  ];

  const mode = $derived(rule ? 'edit' : 'create');

  const canSave = $derived(pattern.trim() !== '' && accountGuid !== '' && !saving && !deleting);

  useDialogReset(
    () => open,
    () => {
      pattern = rule?.pattern ?? '';
      matchType = rule?.matchType ?? 'contains';
      accountGuid = rule?.accountGuid ?? '';
      priority = rule?.priority ?? nextPriority;
      confirmDelete = false;
      saving = false;
      deleting = false;
    }
  );

  function buildInput(): ImportRuleInput {
    return { priority, matchType, pattern: pattern.trim(), accountGuid };
  }

  async function handleSave() {
    if (!canSave) return;
    saving = true;
    try {
      const input = buildInput();
      if (rule) {
        await updateImportRule(rule.id, input);
      } else {
        await createImportRule(input);
      }
      toast.success(rule ? 'Import rule updated' : 'Import rule added');
      open = false;
      onSaved();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to save the rule'));
    } finally {
      saving = false;
    }
  }

  async function handleDelete() {
    if (!rule || saving || deleting) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    deleting = true;
    try {
      await deleteImportRule(rule.id);
      toast.success('Import rule deleted');
      open = false;
      onSaved();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to delete the rule'));
    } finally {
      deleting = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{rule ? 'Edit import rule' : 'New import rule'}</Dialog.Title>
      <Dialog.Description>
        A CSV row whose description matches goes to this account instead of the mapping step's
        account.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-1.5">
        <label for="rule-pattern" class="text-sm font-medium">Description pattern</label>
        <Input
          id="rule-pattern"
          bind:value={pattern}
          placeholder="e.g. Migros"
          maxlength={200}
          autocomplete="off"
        />
      </div>

      <div class="flex flex-col gap-1.5">
        <label for="rule-match" class="text-sm font-medium">Match</label>
        <Select.Root
          type="single"
          value={matchType}
          onValueChange={(v) => {
            if (v === 'contains' || v === 'equals') matchType = v;
          }}
          items={matchItems}
        >
          <Select.Trigger id="rule-match" class="w-full">
            <SelectPrimitive.Value placeholder="Select match type" />
          </Select.Trigger>
          <Select.Content>
            {#each matchItems as m (m.value)}
              <Select.Item value={m.value} label={m.label}>{m.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>

      <div class="flex flex-col gap-1.5">
        <label for="rule-account" class="text-sm font-medium">Category</label>
        <AccountPicker
          id="rule-account"
          bind:value={accountGuid}
          {options}
          placeholder="Select an account"
        />
      </div>

      {#if rule && confirmDelete}
        <p
          role="alert"
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          Delete this rule? Matching rows fall back to the mapping step's account.
        </p>
      {/if}
    </div>

    <SaveFooter
      {mode}
      {saving}
      {deleting}
      {confirmDelete}
      saveLabel={rule ? 'Save changes' : 'Add rule'}
      saveDisabled={!canSave}
      onDelete={handleDelete}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
