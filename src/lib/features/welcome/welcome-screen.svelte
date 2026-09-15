<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { FileDown, FileWarning } from '@lucide/svelte';
  import {
    createBookViaDialog,
    openBookAt,
    openBookViaDialog
  } from '$lib/features/book/book-actions';
  import { toast } from '$lib/components/ui/sonner';
  import { bookDisplayName } from '$lib/utils/book';
  import { getVersion } from '@tauri-apps/api/app';
  import { useFileDrop } from '$lib/hooks/use-file-drop.svelte';
  import WelcomeActions from './welcome-actions.svelte';
  import { isSpendoBookPath } from './welcome-utils';

  let busy = $state<'new' | 'open' | null>(null);
  let appVersion = $state<string | null>(null);

  const drop = useFileDrop(handleDrop);
  const dropBookPath = $derived(drop.paths.find(isSpendoBookPath) ?? null);
  const dropIsGnucash = $derived(
    drop.paths.some((path) => path.toLowerCase().endsWith('.gnucash'))
  );

  onMount(() => {
    getVersion()
      .then((v) => {
        appVersion = v;
      })
      .catch(() => {
        // The version is decorative; the footer just omits it on failure.
      });
  });

  /** `book-actions` navigates nothing itself, so welcome lands on the ledger. */
  async function enterApp() {
    await goto(resolve('/transactions'));
  }

  async function handleNew() {
    if (busy) return;
    busy = 'new';
    try {
      if (await createBookViaDialog()) await enterApp();
    } finally {
      busy = null;
    }
  }

  async function handleOpen() {
    if (busy) return;
    busy = 'open';
    try {
      if (await openBookViaDialog()) await enterApp();
    } finally {
      busy = null;
    }
  }

  async function handleOpenPath(path: string) {
    if (busy) return;
    busy = 'open';
    try {
      if (await openBookAt(path)) await enterApp();
    } finally {
      busy = null;
    }
  }

  function handleDrop(paths: string[]) {
    if (busy) return;
    const path = paths.find(isSpendoBookPath);
    if (!path) {
      if (paths.some((p) => p.toLowerCase().endsWith('.gnucash'))) {
        toast.error('Create a book first, then import this GnuCash file');
      } else {
        toast.error('Only Spendo books can be opened');
      }
      return;
    }
    void handleOpenPath(path);
  }

  /** New/Open reachable from the keyboard while book-less. */
  function handleKeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || busy !== null) return;
    const key = e.key.toLowerCase();
    if (key === 'o') {
      e.preventDefault();
      void handleOpen();
    } else if (key === 'n') {
      e.preventDefault();
      void handleNew();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="relative flex flex-1 flex-col">
  <div class="my-auto flex w-full max-w-2xl flex-col gap-10 self-center py-8">
    <header class="flex flex-col items-center gap-5 text-center">
      <div class="relative">
        <div
          class="absolute inset-3 z-0 rounded-full bg-foreground/5 blur-2xl"
          aria-hidden="true"
        ></div>
        <img
          src="/favicon.svg"
          alt="Spendo application icon"
          class="relative z-10 size-24 rounded-2xl shadow-md"
        />
      </div>
      <div class="flex flex-col gap-3">
        <p class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
          Local-first finance manager
        </p>
        <h1 class="text-4xl font-bold tracking-tight text-balance sm:text-5xl">
          Your books, kept on disk.
        </h1>
        <p class="mx-auto max-w-md text-sm text-pretty text-muted-foreground">
          Browse accounts and transactions, track investments, and review reports. Everything stays
          in portable files on this machine.
        </p>
      </div>
    </header>

    <WelcomeActions {busy} onNew={handleNew} onOpen={handleOpen} />
  </div>

  <footer class="flex justify-center pt-8 font-mono text-xs text-muted-foreground">
    Spendo{appVersion ? ` ${appVersion}` : ''} ·
    <a
      class="rounded-sm underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      href={resolve('/about')}>About</a
    >
  </footer>

  {#if drop.dragging}
    <div
      class="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-xl border-2 border-dashed border-primary/50 bg-background/80 backdrop-blur-sm"
    >
      <div class="flex flex-col items-center gap-3 text-center">
        {#if dropBookPath}
          <FileDown aria-hidden="true" class="size-7 text-primary" />
          <p class="max-w-xs text-sm font-medium">
            Drop to open {bookDisplayName(dropBookPath)}
          </p>
        {:else if dropIsGnucash}
          <FileWarning aria-hidden="true" class="size-7 text-destructive" />
          <p class="max-w-xs text-sm font-medium">
            Create a book first, then import this GnuCash file
          </p>
        {:else}
          <FileWarning aria-hidden="true" class="size-7 text-muted-foreground" />
          <p class="max-w-xs text-sm font-medium">Only .spendo books can be opened</p>
        {/if}
      </div>
    </div>
  {/if}
</div>
