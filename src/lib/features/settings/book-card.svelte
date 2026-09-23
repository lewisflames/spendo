<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { BOOK_CURRENCIES } from '$lib/utils/currency';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { resolve } from '$app/paths';
  import { HardDrive, Copy, Check } from '@lucide/svelte';
  import { dbSession, activeBookPath, setDbSession } from '$lib/stores/db.svelte';
  import { setBaseCurrency } from '$lib/tauri';
  import { copyActiveBookPath } from '$lib/features/book/book-actions';

  let copied = $state(false);
  let savingCurrency = $state(false);
  let currencyDialogOpen = $state(false);
  let pendingCurrency = $state<string | null>(null);

  const activePath = $derived(activeBookPath.value);

  function bookHasData(): boolean {
    const summary = dbSession.value?.summary;
    return !!summary && (summary.totalAccounts > 0 || summary.totalTransactions > 0);
  }

  function requestCurrencyChange(v: string | undefined) {
    if (!v || !dbSession.value || v === dbSession.value.summary.currency) return;
    // Revaluing a populated book touches every report, so confirm first.
    // Empty books can switch silently.
    if (bookHasData()) {
      pendingCurrency = v;
      currencyDialogOpen = true;
    } else {
      void handleCurrencyChange(v);
    }
  }

  async function confirmCurrencyChange() {
    if (!pendingCurrency) return;
    const next = pendingCurrency;
    pendingCurrency = null;
    currencyDialogOpen = false;
    await handleCurrencyChange(next);
  }

  async function handleCurrencyChange(v: string) {
    savingCurrency = true;
    try {
      const result = await setBaseCurrency(v);
      setDbSession(result.summary);
      toast.success(`Base currency set to ${v}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to set base currency'));
    } finally {
      savingCurrency = false;
    }
  }

  async function handleCopyPath() {
    if (await copyActiveBookPath()) {
      copied = true;
      setTimeout(() => (copied = false), 1500);
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Book"
    description="Your book is a portable .spendo file. Open, create, import into or export from it via the menu at the bottom of the sidebar."
  >
    {#snippet icon()}
      <HardDrive aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow
      label="Base currency"
      description="Report currency for the book — revalues net worth, reports and portfolio"
    >
      {#if dbSession.value}
        <Select.Root
          type="single"
          value={dbSession.value.summary.currency}
          onValueChange={requestCurrencyChange}
          items={BOOK_CURRENCIES.map((c) => ({ value: c, label: c }))}
          disabled={savingCurrency}
        >
          <Select.Trigger class="w-36">
            <SelectPrimitive.Value placeholder="Select currency" />
          </Select.Trigger>
          <Select.Content>
            {#each BOOK_CURRENCIES as c (c)}
              <Select.Item value={c} label={c}>{c}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      {:else}
        <span class="text-xs text-muted-foreground">
          No book yet — start from <a href={resolve('/')} class="underline">Welcome</a>.
        </span>
      {/if}
    </SettingSelectRow>
    <div class="flex w-full items-center gap-2">
      <code
        class="max-w-full flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-xs text-muted-foreground"
        title={activePath ?? undefined}
      >
        {activePath ?? 'No book open'}
      </code>
      <Button
        variant="outline"
        size="icon"
        disabled={!activePath}
        onclick={handleCopyPath}
        aria-label="Copy book path"
        title="Copy book path"
      >
        {#if copied}
          <Check aria-hidden="true" class="size-4" />
        {:else}
          <Copy aria-hidden="true" class="size-4" />
        {/if}
      </Button>
    </div>
  </Card.Content>
</Card.Root>

<Dialog.Root bind:open={currencyDialogOpen}>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Change base currency?</Dialog.Title>
      <Dialog.Description>
        This revalues net worth, P&amp;L reports and portfolio totals to {pendingCurrency ?? ''}.
        Past transactions keep their original amounts; only the report currency changes.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          currencyDialogOpen = false;
          pendingCurrency = null;
        }}
        disabled={savingCurrency}
      >
        Cancel
      </Button>
      <Button onclick={confirmCurrencyChange} disabled={savingCurrency || !pendingCurrency}>
        {savingCurrency ? 'Saving…' : `Change to ${pendingCurrency ?? ''}`}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
