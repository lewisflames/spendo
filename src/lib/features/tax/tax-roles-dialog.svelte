<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity';
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { fetchAccounts, fetchTaxRoles, saveTaxRoles, type TaxRole } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import {
    inheritedRole,
    roleRows,
    rolesFor,
    TAX_ROLES,
    toAssignments,
    type RoleRow
  } from './tax-utils';

  let {
    open = $bindable(false),
    onSaved
  }: {
    open: boolean;
    onSaved: () => void;
  } = $props();

  /** Select value for "no role of its own" (bits-ui items need a non-empty value). */
  const NONE = '__none';

  let rows = $state<RoleRow[]>([]);
  const roles = new SvelteMap<string, TaxRole>();
  let loading = $state(false);
  let loadError = $state('');
  let saving = $state(false);
  let error = $state('');

  async function load() {
    loading = true;
    loadError = '';
    try {
      const [accounts, assigned] = await Promise.all([fetchAccounts(), fetchTaxRoles()]);
      rows = roleRows(accounts.trees);
      roles.clear();
      for (const r of assigned) roles.set(r.accountGuid, r.role);
    } catch (e: unknown) {
      loadError = errMsg(e, 'We couldn’t load your accounts');
    } finally {
      loading = false;
    }
  }

  useDialogReset(
    () => open,
    () => {
      saving = false;
      error = '';
      void load();
    }
  );

  function itemsFor(row: RoleRow) {
    return [
      { value: NONE, label: 'No role' },
      ...rolesFor(row.accountType).map((r) => ({ value: r, label: TAX_ROLES[r].label }))
    ];
  }

  function setRole(guid: string, value: string) {
    if (value === NONE) roles.delete(guid);
    else roles.set(guid, value as TaxRole);
  }

  async function save() {
    if (saving) return;
    saving = true;
    error = '';
    try {
      await saveTaxRoles(toAssignments(roles, rows));
      open = false;
      onSaved();
    } catch (e: unknown) {
      error = errMsg(e, 'We couldn’t save the tax roles');
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>Tax roles</Dialog.Title>
      <Dialog.Description>
        Tell Spendo which accounts hold taxable income and the tax taken from it. Sub-accounts
        follow their parent unless they have a role of their own.
      </Dialog.Description>
    </Dialog.Header>

    {#if loadError}
      <ErrorCard message={loadError} onRetry={() => void load()} />
    {:else if loading && rows.length === 0}
      <p class="py-2 text-sm text-muted-foreground">Loading your accounts…</p>
    {:else}
      {#each ['INCOME', 'EXPENSE'] as type (type)}
        {@const group = rows.filter((r) => r.accountType === type)}
        {#if group.length > 0}
          <section class="flex flex-col gap-1">
            <h3 class="text-xs font-medium text-muted-foreground">
              {type === 'INCOME' ? 'Income accounts' : 'Expense accounts'}
            </h3>
            <ul class="flex flex-col">
              {#each group as row (row.guid)}
                {@const own = roles.get(row.guid) ?? null}
                {@const inherited = own ? null : inheritedRole(row.guid, rows, roles)}
                <li class="flex items-center gap-3 border-b py-1.5 last:border-b-0">
                  <span class="min-w-0 flex-1 text-sm" style:padding-left="{row.depth * 1}rem">
                    <AccountLabel name={row.name} code={row.code} />
                  </span>
                  <Select.Root
                    type="single"
                    value={own ?? NONE}
                    onValueChange={(v) => setRole(row.guid, v)}
                    items={itemsFor(row)}
                  >
                    <Select.Trigger class="w-56 shrink-0" aria-label="Tax role for {row.name}">
                      {#if own}
                        <SelectPrimitive.Value />
                      {:else if inherited}
                        <span class="truncate text-muted-foreground">
                          {TAX_ROLES[inherited].label} (from parent)
                        </span>
                      {:else}
                        <span class="text-muted-foreground">No role</span>
                      {/if}
                    </Select.Trigger>
                    <Select.Content>
                      {#each itemsFor(row) as item (item.value)}
                        <Select.Item value={item.value} label={item.label}>
                          <span class="flex flex-col">
                            <span>{item.label}</span>
                            {#if item.value !== NONE}
                              <span class="text-xs text-muted-foreground">
                                {TAX_ROLES[item.value as TaxRole].hint}
                              </span>
                            {/if}
                          </span>
                        </Select.Item>
                      {/each}
                    </Select.Content>
                  </Select.Root>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      {/each}
    {/if}

    {#if error}
      <p
        role="alert"
        class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
      >
        {error}
      </p>
    {/if}

    <SaveFooter
      {saving}
      deleting={false}
      confirmDelete={false}
      saveLabel="Save roles"
      saveDisabled={saving || loading || !!loadError}
      onSave={save}
      onCancel={() => (open = false)}
    />
  </Dialog.Content>
</Dialog.Root>
