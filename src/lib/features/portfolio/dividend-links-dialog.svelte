<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { X } from '@lucide/svelte';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { fetchAccounts, setDividendLinks, type DividendAccount } from '$lib/tauri';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { errMsg } from '$lib/utils/error';

  let {
    open = $bindable(false),
    commodityGuid,
    mnemonic,
    linked,
    onSave
  }: {
    open: boolean;
    commodityGuid: string;
    mnemonic: string;
    linked: DividendAccount[];
    onSave?: () => void;
  } = $props();

  const accountsRes = useAsyncData({
    enabled: () => open,
    fetch: fetchAccounts,
    label: 'We couldn’t load your accounts'
  });

  const incomeOptions = $derived(
    flattenAccountsForPicker(accountsRes.data?.trees, { hidePlaceholders: true }).filter(
      (o) => o.accountType === 'INCOME'
    )
  );

  let selected = $state<string[]>([]);
  let pick = $state('');
  let saving = $state(false);
  let formError = $state('');

  useDialogReset(() => open, sync);

  function sync() {
    selected = linked.map((a) => a.accountGuid);
    pick = '';
    saving = false;
    formError = '';
  }

  let selectedOptions = $derived(
    selected.map(
      (guid) =>
        incomeOptions.find((o) => o.guid === guid) ?? {
          guid,
          label: linked.find((a) => a.accountGuid === guid)?.accountName ?? guid
        }
    )
  );
  let pickable = $derived(incomeOptions.filter((o) => !selected.includes(o.guid)));

  function add(guid: string) {
    if (!selected.includes(guid)) selected = [...selected, guid];
    pick = '';
  }

  async function handleSave() {
    saving = true;
    formError = '';
    try {
      await setDividendLinks({ commodityGuid, accountGuids: selected });
      open = false;
      onSave?.();
    } catch (e: unknown) {
      formError = errMsg(e, 'We couldn’t save these links');
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Dividend accounts for {mnemonic}</Dialog.Title>
      <Dialog.Description>
        Everything posted to these income accounts counts as a {mnemonic} dividend. Payments that already
        touch the {mnemonic} account are picked up without a link.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-3 py-4">
      {#if selectedOptions.length > 0}
        <ul class="flex flex-col gap-1.5">
          {#each selectedOptions as o (o.guid)}
            <li
              class="flex items-center justify-between gap-2 rounded-md border px-3 py-1.5 text-sm"
            >
              <span class="truncate">{o.label}</span>
              <Button
                variant="ghost"
                size="icon"
                class="size-7"
                aria-label="Unlink {o.label}"
                onclick={() => (selected = selected.filter((g) => g !== o.guid))}
              >
                <X aria-hidden="true" class="size-4" />
              </Button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="text-sm text-muted-foreground">No income accounts linked yet.</p>
      {/if}
      <label for="dividend-account-pick" class="text-sm font-medium">Add an income account</label>
      <AccountPicker
        id="dividend-account-pick"
        bind:value={pick}
        options={pickable}
        placeholder="Pick an income account"
        disabled={accountsRes.loading}
        onSelect={add}
      />
      {#if accountsRes.error}
        <p role="alert" class="text-sm text-destructive">{accountsRes.error}</p>
      {/if}
      {#if formError}
        <p role="alert" class="text-sm text-destructive">{formError}</p>
      {/if}
    </div>

    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
      <Button onclick={handleSave} disabled={saving}>{saving ? 'Saving…' : 'Save'}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
