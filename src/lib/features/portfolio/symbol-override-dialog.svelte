<script lang="ts">
  import { untrack } from 'svelte';
  import * as Dialog from '$lib/components/ui/dialog';
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
  let symbolError = $state('');

  // Re-initialize whenever the dialog opens so it always reflects the saved
  // override. $effect.pre (not onOpenChange): bits-ui doesn't fire open
  // changes for programmatic bind:open, the only way this dialog opens.
  $effect.pre(() => {
    if (open) untrack(() => syncFromEntry());
  });

  function syncFromEntry() {
    symbol = currentSymbol ?? '';
    symbolError = '';
  }

  async function handleSave() {
    const trimmed = symbol.trim();
    symbolError = trimmed ? '' : 'Enter a Yahoo symbol (e.g. AAPL, SIE.DE, BTC-EUR)';
    if (!trimmed) return;
    loading = true;
    try {
      await setSymbolOverride(commodityGuid, trimmed);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to save Yahoo symbol'));
    } finally {
      loading = false;
    }
  }

  async function handleReset() {
    resetting = true;
    try {
      await deleteSymbolOverride(commodityGuid);
      open = false;
      onSave?.();
    } catch (e: unknown) {
      onError?.(errMsg(e, 'Failed to reset Yahoo symbol'));
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
      />
      {#if symbolError}
        <p id="yahoo-symbol-error" role="alert" class="text-xs text-destructive">{symbolError}</p>
      {/if}
      <p class="text-xs text-muted-foreground">
        Saved in uppercase. Refresh prices afterwards to fetch with the new symbol.
      </p>
    </div>

    <Dialog.Footer class="sm:justify-between">
      <div>
        {#if currentSymbol}
          <Button variant="ghost" onclick={handleReset} disabled={loading || resetting}>
            {resetting ? 'Resetting...' : 'Reset to automatic'}
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
        <Button onclick={handleSave} disabled={loading || resetting}>
          {loading ? 'Saving...' : 'Save'}
        </Button>
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
