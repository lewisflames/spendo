<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';

  import { setBudget } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import type { AccountsResult } from '$lib/tauri';

  let {
    open = $bindable(false),
    accountsData = null,
    budgetedAccountGuids = [],
    currency = 'EUR',
    onSave,
    onError
  }: {
    open: boolean;
    accountsData?: AccountsResult | null;
    budgetedAccountGuids?: string[];
    currency?: string;
    onSave?: (accountGuid: string) => void;
    onError?: (msg: string) => void;
  } = $props();

  interface AccountTreeNode {
    guid: string;
    name: string;
    description: string | null;
    accountType: string;
    placeholder: boolean;
    children: AccountTreeNode[];
  }

  function flattenExpenseAccounts(
    nodes: AccountTreeNode[]
  ): { guid: string; name: string; description: string | null }[] {
    const result: { guid: string; name: string; description: string | null }[] = [];
    function walk(list: AccountTreeNode[]) {
      for (const n of list) {
        if (!n.placeholder && n.accountType === 'EXPENSE') {
          result.push({ guid: n.guid, name: n.name, description: n.description });
        }
        if (n.children.length > 0) walk(n.children);
      }
    }
    walk(nodes);
    return result;
  }

  // Only accounts without a budget can be picked — every account gets one.
  let addAccounts = $derived(
    accountsData?.trees
      ? flattenExpenseAccounts(accountsData.trees).filter(
          (a) => !budgetedAccountGuids.includes(a.guid)
        )
      : []
  );

  let addAccountGuid = $state('');
  let addAmount = $state(0);
  let addNotes = $state('');

  let addLoading = $state(false);
  let addAccountError = $state('');
  let addAmountError = $state('');

  function resetForm() {
    addAccountGuid = '';
    addAmount = 0;
    addNotes = '';
    addAccountError = '';
    addAmountError = '';
  }

  async function handleSave() {
    addAccountError = addAccountGuid ? '' : 'Please select an expense account';
    addAmountError = addAmount > 0 ? '' : 'Target must be greater than zero';
    if (!addAccountGuid || addAmount <= 0) return;
    addLoading = true;
    try {
      await setBudget(addAccountGuid, addNotes.trim() || undefined, addAmount);
      const savedGuid = addAccountGuid;
      open = false;
      resetForm();
      onSave?.(savedGuid);
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to add budget'));
    } finally {
      addLoading = false;
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={(o) => !o && resetForm()}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Add Budget</Dialog.Title>
      <Dialog.Description>
        Set a monthly spending target for an expense account. The target applies to every month and
        can be adjusted per period afterwards.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-4 py-4">
      <div class="flex flex-col gap-2">
        <label for="account" class="text-sm font-medium">Account</label>
        <Select.Root
          type="single"
          bind:value={addAccountGuid}
          items={addAccounts.map((a) => ({ value: a.guid, label: a.description || a.name }))}
        >
          <Select.Trigger
            id="account"
            class="w-full"
            aria-invalid={!!addAccountError}
            aria-describedby={addAccountError ? 'account-error' : undefined}
          >
            <SelectPrimitive.Value placeholder="Select an expense account" />
          </Select.Trigger>
          <Select.Content>
            {#each addAccounts as a (a.guid)}
              <Select.Item value={a.guid} label={a.description || a.name}
                >{a.description || a.name}</Select.Item
              >
            {/each}
          </Select.Content>
        </Select.Root>
        {#if addAccountError}
          <p id="account-error" role="alert" class="text-xs text-destructive">{addAccountError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="amount" class="text-sm font-medium">Monthly target</label>
        <div class="relative">
          <Input
            id="amount"
            type="number"
            step="0.01"
            min="0"
            bind:value={addAmount}
            placeholder="0.00"
            class="pr-16"
            aria-invalid={!!addAmountError}
            aria-describedby={addAmountError ? 'amount-error' : undefined}
          />
          <span
            class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-muted-foreground uppercase"
            >{currency}</span
          >
        </div>
        {#if addAmountError}
          <p id="amount-error" role="alert" class="text-xs text-destructive">{addAmountError}</p>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <label for="notes" class="text-sm font-medium">Notes</label>
        <Textarea
          id="notes"
          bind:value={addNotes}
          placeholder="Optional notes about this budget"
          class="min-h-20"
        />
      </div>
    </div>

    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button onclick={handleSave} disabled={addLoading}>
        {addLoading ? 'Saving...' : 'Save Budget'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
