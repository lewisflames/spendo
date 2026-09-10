<script lang="ts">
  import { Button, type ButtonProps } from '$lib/components/ui/button';
  import { Upload, LoaderCircle } from '@lucide/svelte';
  import { setDbSession } from '$lib/stores/db.svelte';
  import { importGnucash } from '$lib/tauri';
  import { toastPriceResult } from '$lib/features/portfolio/price-utils';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { open } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';

  let {
    label = 'Import from GnuCash',
    disabled = false,
    onImported,
    ...rest
  }: {
    label?: string;
    disabled?: boolean;
    onImported?: () => void;
  } & Omit<ButtonProps, 'onclick' | 'disabled'> = $props();

  let importing = $state(false);

  async function handleImport() {
    importing = true;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'GnuCash', extensions: ['gnucash'] }]
      });
      if (!selected) return;
      const filename = await basename(selected);
      const result = await importGnucash(selected, filename);
      setDbSession(result.summary);
      toast.success(`Imported ${filename}`);
      toastPriceResult(result.prices, { fixHint: true });
      onImported?.();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Import failed'));
    } finally {
      importing = false;
    }
  }
</script>

<Button onclick={handleImport} disabled={importing || disabled} {...rest}>
  {#if importing}
    <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
  {:else}
    <Upload aria-hidden="true" class="size-4" />
  {/if}
  {label}
</Button>
