<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { Input } from '$lib/components/ui/input';
  import { Button } from '$lib/components/ui/button';

  import { setSymbolOverride, deleteSymbolOverride } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';

  let {
    open = $bindable(false),
    commodityGuid,
    mnemonic,
    currentSymbol,
    onSave,
    onError
  }: {
    open: boolean;
    commodityGuid: string;
    mnemonic: string;
    currentSymbol: string | null;
    onSave?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let symbol = $state('');
  let loading = $state(false);
  let resetting = $state(false);
  let confirmReset = $state(false);
  let formError = $state('');

  // Re-initialize whenever the dialog opens so it always reflects the saved
  // override.
  useDialogReset(() => open, syncFromEntry);

  function syncFromEntry() {
    symbol = currentSymbol ?? '';
    formError = '';
    confirmReset = false;
    loading = false;
    resetting = false;
  }

  let symbolError = $derived(
    symbol.trim() ? '' : 'Enter a Yahoo symbol (e.g. AAPL, SIE.DE, BTC-EUR)'
  );
  let saveDisabled = $derived(loading || resetting || !symbol.trim());

  async function handleSave() {
    const trimmed = symbol.trim();
    if (!trimmed) return;
    loading = true;
    formError = '';
    try {
      await setSymbolOverride(commodityGuid, trimmed);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save Yahoo symbol');
      formError = msg;
      onError?.(msg);
    } finally {
      loading = false;
    }
  }

  async function handleReset() {
    if (!confirmReset) {
      confirmReset = true;
      return;
    }
    resetting = true;
    formError = '';
    try {
      await deleteSymbolOverride(commodityGuid);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to reset Yahoo symbol');
      formError = msg;
      onError?.(msg);
    } finally {
      resetting = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Yahoo symbol for {mnemonic}</Dialog.Title>
      <Dialog.Description>
        {#if currentSymbol}
          Override in use: <span class="font-mono font-medium">{currentSymbol}</span>. Prices are
          fetched for this symbol only.
        {:else}
          Automatic detection is in use. Set an override to fetch a specific Yahoo Finance symbol.
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-2 py-4">
      <label for="yahoo-symbol" class="text-sm font-medium">Yahoo symbol</label>
      <Input
        id="yahoo-symbol"
        bind:value={symbol}
        placeholder="e.g. AAPL, SIE.DE, BTC-EUR"
        class="font-mono"
        aria-invalid={!!symbolError}
        aria-describedby={symbolError ? 'yahoo-symbol-error' : undefined}
        autofocus
      />
      {#if symbolError}
        <p id="yahoo-symbol-error" role="alert" class="text-xs text-destructive">{symbolError}</p>
      {/if}
      {#if formError}
        <p role="alert" class="text-sm text-destructive">{formError}</p>
      {/if}
      {#if confirmReset}
        <p role="alert" class="text-sm text-destructive">
          Resetting removes the override and returns to automatic detection. Click reset again to
          confirm.
        </p>
      {:else}
        <p class="text-xs text-muted-foreground">
          Saved in uppercase. Refresh prices afterwards to fetch with the new symbol.
        </p>
      {/if}
    </div>

    <Dialog.Footer class="sm:justify-between">
      <div>
        {#if currentSymbol}
          <Button
            variant={confirmReset ? 'destructive' : 'ghost'}
            onclick={handleReset}
            disabled={loading || resetting}
          >
            {resetting ? 'Resetting…' : confirmReset ? 'Confirm reset' : 'Reset to automatic'}
          </Button>
        {/if}
      </div>
      <div class="flex gap-2">
        <Button
          variant="outline"
          onclick={() => {
            open = false;
          }}>Cancel</Button
        >
        <Button onclick={handleSave} disabled={saveDisabled}>
          {loading ? 'Saving…' : 'Save'}
        </Button>
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
