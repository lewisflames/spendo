<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { FolderPlus, FolderOpen, LoaderCircle } from '@lucide/svelte';
  import { setActiveBookPath, setDbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import { BOOK_CURRENCIES, defaultCurrencyForLocale } from '$lib/utils/currency';
  import { ensureSpendoExtension } from '$lib/utils/book';
  import { createBookAt, openBook } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import WelcomeBackground from './welcome-background.svelte';

  let currency = $state(defaultCurrencyForLocale(appSettings.value.locale));
  let busy = $state<'new' | 'open' | null>(null);
  let error = $state<string | null>(null);

  async function handleNew() {
    if (busy) return;
    error = null;
    let target: string | null;
    try {
      target = await save({
        defaultPath: 'book.spendo',
        filters: [{ name: 'Spendo book', extensions: ['spendo'] }]
      });
    } catch (e: unknown) {
      error = errMsg(e, 'Failed to open save dialog');
      return;
    }
    if (!target) return;
    const file = ensureSpendoExtension(target);
    busy = 'new';
    try {
      const result = await createBookAt(file, currency);
      setDbSession(result.summary);
      setActiveBookPath(file);
      toast.success(`Book created (${currency})`);
      await goto(resolve('/transactions'));
    } catch (e: unknown) {
      error = errMsg(e, 'Failed to create book');
    } finally {
      busy = null;
    }
  }

  async function handleOpen() {
    if (busy) return;
    error = null;
    let selected: string | string[] | null;
    try {
      selected = await open({
        multiple: false,
        filters: [{ name: 'Spendo book', extensions: ['spendo', 'db', 'sqlite'] }]
      });
    } catch (e: unknown) {
      error = errMsg(e, 'Failed to open file dialog');
      return;
    }
    if (!selected || Array.isArray(selected)) return;
    busy = 'open';
    try {
      const result = await openBook(selected);
      setDbSession(result.summary);
      setActiveBookPath(selected);
      toast.success(`Opened ${await basename(selected)}`);
      await goto(resolve('/transactions'));
    } catch (e: unknown) {
      error = errMsg(e, 'Failed to open book');
    } finally {
      busy = null;
    }
  }
</script>

<section class="relative flex min-h-full flex-col items-center justify-center gap-8 px-6 py-16">
  <WelcomeBackground />

  <div class="relative z-10 flex flex-col items-center gap-6 text-center">
    <img src="/favicon.svg" alt="Spendo application icon" class="size-24 rounded-2xl shadow-md" />
    <div class="flex flex-col items-center gap-3">
      <p class="text-xs font-medium tracking-widest text-muted-foreground uppercase">
        Local-first finance manager
      </p>
      <h1 class="max-w-xl text-4xl font-bold tracking-tight text-balance sm:text-5xl">
        See your finances clearly
      </h1>
      <p class="max-w-md text-lg text-pretty text-muted-foreground">
        Browse accounts and transactions, track investments, and review reports. Your data stays on
        this machine.
      </p>
    </div>
  </div>

  {#if error}
    <div class="relative z-10 w-full max-w-md">
      <ErrorCard message={error} />
    </div>
  {/if}

  <div class="relative z-10 w-full max-w-md">
    <Card.Root>
      <Card.Header>
        <Card.Title>Open or create a book</Card.Title>
        <Card.Description>
          Books are portable .spendo files stored wherever you choose. Using GnuCash? Create or open
          a book first, then bring it in via Import on the Transactions page.
        </Card.Description>
      </Card.Header>
      <Card.Content class="flex flex-1 flex-col gap-3">
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-1">
            <span class="text-sm font-medium">Base currency</span>
            <span class="text-xs text-muted-foreground">For new books; changeable later</span>
          </div>
          <Select.Root
            type="single"
            value={currency}
            onValueChange={(v) => {
              if (v) currency = v;
            }}
            items={BOOK_CURRENCIES.map((c) => ({ value: c, label: c }))}
            disabled={busy !== null}
          >
            <Select.Trigger class="w-36">
              <SelectPrimitive.Value placeholder="Currency" />
            </Select.Trigger>
            <Select.Content>
              {#each BOOK_CURRENCIES as c (c)}
                <Select.Item value={c} label={c}>{c}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <Button size="lg" onclick={handleNew} disabled={busy !== null} class="w-full">
          {#if busy === 'new'}
            <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          {:else}
            <FolderPlus aria-hidden="true" class="size-4" />
          {/if}
          {busy === 'new' ? 'Creating...' : 'New book...'}
        </Button>
        <Button
          size="lg"
          variant="outline"
          onclick={handleOpen}
          disabled={busy !== null}
          class="w-full"
        >
          {#if busy === 'open'}
            <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          {:else}
            <FolderOpen aria-hidden="true" class="size-4" />
          {/if}
          {busy === 'open' ? 'Opening...' : 'Open book...'}
        </Button>
      </Card.Content>
    </Card.Root>
  </div>
</section>
