<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import DialogSaveFooter from '$lib/components/shared/dialog-save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import {
    createAccount,
    deleteAccount,
    fetchAccounts,
    fetchBookCurrencies,
    updateAccount,
    type AccountInput,
    type AccountsResult,
    type BookCurrency
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { ACCOUNT_TYPES } from './account-types';
  import { flattenAccountsForPicker, subtreeGuids } from './account-picker-utils';

  export interface AccountInitial {
    name: string;
    accountType: string;
    parentGuid: string | null;
    code: string | null;
    description: string | null;
    commodityGuid: string | null;
    placeholder: boolean;
  }

  let {
    open = $bindable(false),
    mode = 'create',
    guid = null,
    initial = null,
    parentPreset = null,
    onSaved,
    onDeleted,
    onError
  }: {
    open: boolean;
    mode?: 'create' | 'edit';
    guid?: string | null;
    initial?: AccountInitial | null;
    parentPreset?: string | null;
    onSaved?: (guid: string) => void;
    onDeleted?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let name = $state('');
  let accountType = $state('BANK');
  let parentGuid = $state('');
  let code = $state('');
  let description = $state('');
  let commodityGuid = $state('');
  let placeholder = $state(false);

  let saving = $state(false);
  let deleting = $state(false);
  let confirmDelete = $state(false);
  let formError = $state('');

  let accountsData = $state<AccountsResult | null>(null);
  let currencies = $state<BookCurrency[]>([]);
  let loading = $state(false);

  let excludedGuids = $derived.by(() => {
    if (mode === 'edit' && guid && accountsData?.trees) {
      return subtreeGuids(accountsData.trees, guid);
    }
    return new Set<string>();
  });

  let parentOptions = $derived(
    flattenAccountsForPicker(accountsData?.trees, { excludeGuids: excludedGuids })
  );

  let descendantCount = $derived(
    mode === 'edit' && guid && accountsData?.trees
      ? Math.max(0, subtreeGuids(accountsData.trees, guid).size - 1)
      : 0
  );

  function syncDefaults() {
    formError = '';
    confirmDelete = false;
    if (mode === 'edit' && initial) {
      name = initial.name;
      accountType = initial.accountType;
      parentGuid = initial.parentGuid ?? '';
      code = initial.code ?? '';
      description = initial.description ?? '';
      commodityGuid = initial.commodityGuid ?? '';
      placeholder = initial.placeholder;
    } else {
      name = '';
      accountType = 'BANK';
      parentGuid = parentPreset ?? '';
      code = '';
      description = '';
      commodityGuid = '';
      placeholder = false;
    }
  }

  useDialogReset(() => open, syncDefaults);

  $effect(() => {
    if (!open) return;
    let cancelled = false;
    loading = true;
    Promise.all([fetchAccounts(), fetchBookCurrencies()])
      .then(([a, c]) => {
        if (cancelled) return;
        accountsData = a;
        currencies = c;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        onError?.(errMsg(e, 'Failed to load accounts'));
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  let nameError = $derived(!name.trim() ? 'Name is required' : '');
  let typeError = $derived(ACCOUNT_TYPES.includes(accountType) ? '' : 'Pick an account type');
  let canSave = $derived(!saving && !deleting && !nameError && !typeError && !loading);

  function buildInput(): AccountInput {
    return {
      name: name.trim(),
      accountType,
      parentGuid: parentGuid || null,
      code: code.trim() ? code.trim() : null,
      description: description.trim() ? description.trim() : null,
      commodityGuid: commodityGuid || null,
      placeholder
    };
  }

  async function handleSave() {
    if (!canSave) return;
    saving = true;
    formError = '';
    try {
      const input = buildInput();
      if (mode === 'edit' && guid) {
        await updateAccount(guid, input);
        open = false;
        onSaved?.(guid);
      } else {
        const newGuid = await createAccount(input);
        open = false;
        onSaved?.(newGuid);
      }
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save account');
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
      await deleteAccount(guid);
      open = false;
      onDeleted?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to delete account');
      formError = msg;
      onError?.(msg);
    } finally {
      deleting = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{mode === 'edit' ? 'Edit account' : 'New account'}</Dialog.Title>
      <Dialog.Description>
        {#if mode === 'edit' && descendantCount > 0}
          Changing the parent moves {descendantCount}
          {descendantCount === 1 ? 'sub-account' : 'sub-accounts'} along with it.
        {:else}
          Names must be unique among siblings.
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-2">
      <div class="flex flex-col gap-1.5">
        <label for="acct-name" class="text-sm font-medium">Name</label>
        <Input
          id="acct-name"
          bind:value={name}
          placeholder="e.g. Groceries"
          maxlength={200}
          aria-invalid={!!nameError}
        />
        {#if nameError}
          <p role="alert" class="text-xs text-destructive">{nameError}</p>
        {/if}
      </div>

      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="flex flex-col gap-1.5">
          <label for="acct-type" class="text-sm font-medium">Type</label>
          <Select.Root type="single" bind:value={accountType}>
            <Select.Trigger id="acct-type" class="w-full">
              <SelectPrimitive.Value placeholder="Select a type" />
            </Select.Trigger>
            <Select.Content>
              {#each ACCOUNT_TYPES as t (t)}
                <Select.Item value={t} label={t}>{t}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          {#if typeError}
            <p role="alert" class="text-xs text-destructive">{typeError}</p>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="acct-currency" class="text-sm font-medium">Currency (optional)</label>
          <Select.Root type="single" bind:value={commodityGuid}>
            <Select.Trigger id="acct-currency" class="w-full">
              <SelectPrimitive.Value placeholder="None" />
            </Select.Trigger>
            <Select.Content>
              <Select.Item value="" label="None">None</Select.Item>
              {#each currencies as c (c.guid)}
                <Select.Item value={c.guid} label={c.mnemonic}>{c.mnemonic}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
      </div>

      <div class="flex flex-col gap-1.5">
        <label for="acct-parent" class="text-sm font-medium">Parent (empty = top level)</label>
        <AccountPicker
          id="acct-parent"
          bind:value={parentGuid}
          options={parentOptions}
          placeholder={loading ? 'Loading accounts…' : 'Top level'}
          disabled={loading}
        />
      </div>

      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="flex flex-col gap-1.5">
          <label for="acct-code" class="text-sm font-medium">Code (optional)</label>
          <Input id="acct-code" bind:value={code} placeholder="e.g. 6000" maxlength={100} />
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="acct-desc" class="text-sm font-medium">Description (optional)</label>
          <Input
            id="acct-desc"
            bind:value={description}
            placeholder="Display name"
            maxlength={500}
          />
        </div>
      </div>

      <label class="flex cursor-pointer items-center justify-between gap-3 rounded-lg border p-3">
        <span class="text-sm">
          <span class="font-medium">Placeholder</span>
          <span class="block text-xs text-muted-foreground">
            Grouping account: no splits directly, totals roll up from children.
          </span>
        </span>
        <Switch bind:checked={placeholder} />
      </label>

      {#if formError}
        <p role="alert" class="text-sm text-destructive">{formError}</p>
      {/if}

      {#if mode === 'edit' && confirmDelete}
        <p
          role="alert"
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          Delete this account? Only empty accounts (no splits, sub-accounts or budgets) can be
          deleted.
        </p>
      {/if}
    </div>

    <DialogSaveFooter
      {mode}
      {saving}
      {deleting}
      {confirmDelete}
      saveLabel={mode === 'edit' ? 'Save changes' : 'Create account'}
      saveDisabled={!canSave}
      onDelete={handleDelete}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
