<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
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
  import { sessionCurrency } from '$lib/utils/format.svelte';
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
  let showErrors = $state(false);
  // Create forms opened before the currency list resolved still need the
  // report-currency default applied once `currencies` arrives.
  let currencyDefaultPending = $state(false);

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

  let currencyItems = $derived([
    { value: '', label: 'None' },
    ...currencies.map((c) => ({ value: c.guid, label: c.mnemonic }))
  ]);

  let descendantCount = $derived(
    mode === 'edit' && guid && accountsData?.trees
      ? Math.max(0, subtreeGuids(accountsData.trees, guid).size - 1)
      : 0
  );

  function syncDefaults() {
    formError = '';
    confirmDelete = false;
    saving = false;
    deleting = false;
    loading = false;
    showErrors = false;
    if (mode === 'edit' && initial) {
      name = initial.name;
      accountType = initial.accountType;
      parentGuid = initial.parentGuid ?? '';
      code = initial.code ?? '';
      description = initial.description ?? '';
      commodityGuid = initial.commodityGuid ?? '';
      placeholder = initial.placeholder;
      currencyDefaultPending = false;
    } else {
      name = '';
      accountType = 'BANK';
      parentGuid = parentPreset ?? '';
      code = '';
      description = '';
      commodityGuid = reportCurrencyGuid();
      placeholder = false;
      currencyDefaultPending = commodityGuid === '';
    }
  }

  /** GUID of the book's report currency, or '' when the list isn't loaded yet. */
  function reportCurrencyGuid(): string {
    const code = sessionCurrency();
    return currencies.find((c) => c.mnemonic === code)?.guid ?? '';
  }

  /**
   * The currency list loads asynchronously after the dialog opens, so a create
   * form opened before it arrives still has no default; apply the report
   * currency once the list resolves. The pending flag keeps this from
   * overriding a currency the user picked in the meantime.
   */
  function applyCurrencyDefault(): void {
    if (mode !== 'create' || !currencyDefaultPending) return;
    const guid = reportCurrencyGuid();
    if (!guid) return;
    commodityGuid = guid;
    currencyDefaultPending = false;
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
        applyCurrencyDefault();
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        onError?.(errMsg(e, "We couldn't load your accounts. Please try again"));
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  let nameError = $derived(!name.trim() ? 'Give this account a name' : '');
  let typeError = $derived(ACCOUNT_TYPES.includes(accountType) ? '' : 'Choose an account type');
  let currencyError = $derived(!placeholder && !commodityGuid ? 'Choose a currency' : '');
  let canSave = $derived(
    !saving && !deleting && !nameError && !typeError && !currencyError && !loading
  );

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
        await updateAccount(guid, input);
        open = false;
        onSaved?.(guid);
      } else {
        const newGuid = await createAccount(input);
        open = false;
        onSaved?.(newGuid);
      }
    } catch (e: unknown) {
      const msg = errMsg(e, "We couldn't save this account. Please try again");
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
      const msg = errMsg(e, "We couldn't delete this account. Please try again");
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
          Moving this also moves {descendantCount}
          {descendantCount === 1 ? 'sub-account' : 'sub-accounts'} with it.
        {:else}
          Sibling accounts can't share a name.
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-1.5">
        <label for="acct-name" class="text-sm font-medium">Name</label>
        <Input
          id="acct-name"
          bind:value={name}
          placeholder="e.g. Groceries"
          maxlength={200}
          aria-invalid={showErrors && !!nameError}
        />
        {#if showErrors && nameError}
          <p role="alert" class="text-xs text-destructive">{nameError}</p>
        {/if}
      </div>

      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div class="flex flex-col gap-1.5">
          <label for="acct-type" class="text-sm font-medium">Type</label>
          <Select.Root type="single" bind:value={accountType}>
            <Select.Trigger id="acct-type" class="w-full" aria-invalid={showErrors && !!typeError}>
              <SelectPrimitive.Value placeholder="Select a type" />
            </Select.Trigger>
            <Select.Content>
              {#each ACCOUNT_TYPES as t (t)}
                <Select.Item value={t} label={t}>{t}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          {#if showErrors && typeError}
            <p role="alert" class="text-xs text-destructive">{typeError}</p>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          <label for="acct-currency" class="text-sm font-medium">Currency</label>
          <Select.Root type="single" bind:value={commodityGuid} items={currencyItems}>
            <Select.Trigger
              id="acct-currency"
              class="w-full"
              aria-invalid={showErrors && !!currencyError}
            >
              <SelectPrimitive.Value placeholder="None" />
            </Select.Trigger>
            <Select.Content>
              <Select.Item value="" label="None">None</Select.Item>
              {#each currencies as c (c.guid)}
                <Select.Item value={c.guid} label={c.mnemonic}>{c.mnemonic}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          {#if showErrors && currencyError}
            <p role="alert" class="text-xs text-destructive">{currencyError}</p>
          {/if}
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
            Just for grouping. Totals roll up from below, and it can't hold money itself.
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
          Delete this account? It needs to be empty first: no transactions, sub-accounts or budgets
          attached.
        </p>
      {/if}
    </div>

    <SaveFooter
      {mode}
      {saving}
      {deleting}
      {confirmDelete}
      saveLabel={mode === 'edit' ? 'Save changes' : 'Create account'}
      saveDisabled={saving || deleting || loading}
      onDelete={handleDelete}
      onSave={handleSave}
      onCancel={() => {
        open = false;
      }}
    />
  </Dialog.Content>
</Dialog.Root>
