<script lang="ts">
  import { Button, type ButtonProps } from '$lib/components/ui/button';
  import { Upload, LoaderCircle } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { importGnucash } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { open } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';

  let {
    label = 'Import',
    onError,
    onStatusChange,
    ...rest
  }: {
    label?: string;
    onError?: (msg: string) => void;
    onStatusChange?: (importing: boolean) => void;
  } & Omit<ButtonProps, 'onclick' | 'disabled'> = $props();

  let importing = $state(false);

  function setImporting(value: boolean) {
    importing = value;
    onStatusChange?.(value);
  }

  async function handleImport() {
    setImporting(true);
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'GnuCash', extensions: ['gnucash'] }]
      });
      if (!selected) return;
      const filename = await basename(selected);
      const result = await importGnucash(selected, filename);
      dbSession.value = { summary: result.summary };
      toast.success(`Imported ${filename}`);
      toast.info(
        'Prices are not imported — refresh prices in Portfolio to load them from Yahoo Finance.'
      );
    } catch (e: unknown) {
      const msg = errMsg(e, 'Import failed');
      if (onError) {
        onError(msg);
      } else {
        toast.error(msg);
      }
    } finally {
      setImporting(false);
    }
  }
</script>

<Button onclick={handleImport} disabled={importing} {...rest}>
  {#if importing}
    <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
  {:else}
    <Upload aria-hidden="true" class="size-4" />
  {/if}
  {label}
</Button>
