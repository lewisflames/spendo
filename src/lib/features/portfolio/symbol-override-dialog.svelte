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
    onSave
  }: {
    open: boolean;
    commodityGuid: string;
    mnemonic: string;
    currentSymbol: string | null;
    onSave?: () => void;
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
    touched = false;
    confirmReset = false;
    loading = false;
    resetting = false;
  }

  let touched = $state(false);
  let symbolError = $derived(
    touched && !symbol.trim() ? 'Enter a Yahoo symbol (e.g. AAPL, SIE.DE, BTC-EUR)' : ''
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
      formError = errMsg(e, 'We couldn’t save this symbol');
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
      formError = errMsg(e, 'We couldn’t reset this symbol');
    } finally {
      resetting = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Which Yahoo symbol is {mnemonic}?</Dialog.Title>
      <Dialog.Description>
        {#if currentSymbol}
          Right now we’re using <span class="font-mono font-medium">{currentSymbol}</span> for prices.
        {:else}
          We’re guessing the symbol for now. Tell us the exact one and we’ll use that instead.
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
        oninput={() => (touched = true)}
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
        <p
          role="alert"
          class="rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
        >
          This forgets your pick and goes back to guessing. Hit reset again if you’re sure.
        </p>
      {:else}
        <p class="text-xs text-muted-foreground">
          We’ll save it in uppercase. Refresh prices after to use it right away.
        </p>
      {/if}
    </div>

    <Dialog.Footer class="sm:justify-between">
      <div>
        {#if currentSymbol}
          <Button
            variant={confirmReset ? 'destructive' : 'outline'}
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
