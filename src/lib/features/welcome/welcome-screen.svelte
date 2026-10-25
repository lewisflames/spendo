<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { ChartLine, FileDown, FileWarning, Import, TrendingUp } from '@lucide/svelte';
  import {
    createBookViaDialog,
    openBookAt,
    openBookViaDialog
  } from '$lib/features/book/book-actions';
  import { toast } from '$lib/components/ui/sonner';
  import { bookDisplayName } from '$lib/utils/book';
  import { getVersion } from '@tauri-apps/api/app';
  import { useFileDrop } from '$lib/hooks/use-file-drop.svelte';
  import { recentBooks } from '$lib/stores/recent-books.svelte';
  import RecentBooksList from './recent-books-list.svelte';
  import WelcomeActions from './welcome-actions.svelte';
  import { isSpendoBookPath } from './welcome-utils';

  const FEATURES = [
    {
      icon: ChartLine,
      title: 'Spending, P&L and tax reports',
      detail: 'Monthly totals with month-end FX'
    },
    {
      icon: TrendingUp,
      title: 'Investments',
      detail: 'FIFO or average cost, returns and dividends'
    },
    { icon: Import, title: 'Bring your history', detail: 'GnuCash books and bank CSV files' }
  ];

  const inTauri = isTauri();

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

  /** `book-actions` navigates nothing itself, so welcome lands on the Overview. */
  async function enterApp() {
    await goto(resolve('/'));
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
        toast.error('Open a book before importing a GnuCash file');
      } else {
        toast.error('Not a Spendo book');
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

<div class="relative grid flex-1 gap-(--page-gap) @3xl:grid-cols-[5fr_6fr]">
  <aside
    class="relative flex flex-col gap-8 overflow-hidden rounded-xl border bg-muted/40 p-6 @3xl:p-10"
  >
    <!-- Faint ledger ruling in the empty lower half: decoration only. -->
    <div
      class="pointer-events-none absolute inset-0 bg-[repeating-linear-gradient(to_bottom,transparent_0_31px,var(--color-border)_31px_32px)] opacity-70"
      style="mask-image: linear-gradient(to bottom, transparent 45%, black 90%)"
      aria-hidden="true"
    ></div>

    <div class="relative flex items-center gap-3">
      <img src="/favicon.svg" alt="" class="size-10" />
      <span class="text-2xl font-semibold tracking-tighter">spendo</span>
    </div>

    <div class="relative flex flex-col gap-3">
      <h1 class="text-3xl font-bold tracking-tight text-balance @5xl:text-4xl">
        Your money, kept on this machine.
      </h1>
      <p class="max-w-sm text-sm text-pretty text-muted-foreground">
        One portable book file holds every account, budget and holding. Nothing leaves your computer
        unless you export it.
      </p>
    </div>

    <ul class="relative hidden flex-col gap-4 @3xl:flex">
      {#each FEATURES as feature (feature.title)}
        <li class="flex gap-3">
          <feature.icon aria-hidden="true" class="mt-0.5 size-4 shrink-0 text-primary" />
          <div class="flex flex-col">
            <span class="text-sm font-medium">{feature.title}</span>
            <span class="text-xs text-muted-foreground">{feature.detail}</span>
          </div>
        </li>
      {/each}
    </ul>

    <footer class="relative mt-auto font-mono text-xs text-muted-foreground">
      Spendo{appVersion ? ` ${appVersion}` : ''} ·
      <a
        class="rounded-sm underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        href={resolve('/about')}>About</a
      >
    </footer>
  </aside>

  <section
    class="@container mx-auto flex w-full max-w-md flex-col justify-center gap-6 py-6 @3xl:py-10"
  >
    <h2 class="text-lg font-semibold tracking-tight">
      {recentBooks.value.length > 0 ? 'Welcome back' : 'Get started'}
    </h2>

    <WelcomeActions {busy} onNew={handleNew} onOpen={handleOpen} />

    {#if recentBooks.value.length > 0}
      <div class="flex flex-col gap-2">
        <h3 class="text-xs font-medium text-muted-foreground">Recent books</h3>
        <RecentBooksList disabled={busy !== null} onOpen={handleOpenPath} />
      </div>
    {/if}

    {#if inTauri}
      <div
        class="flex items-center gap-3 rounded-xl border-2 border-dashed p-4 text-sm text-muted-foreground"
      >
        <FileDown aria-hidden="true" class="size-5 shrink-0" />
        <div class="flex flex-col">
          <span class="font-medium text-foreground">Drop a book here</span>
          <span class="text-xs">
            Or anywhere in this window. GnuCash files come in once a book is open.
          </span>
        </div>
      </div>
    {/if}
  </section>

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
            Start a book first, then bring in that GnuCash file
          </p>
        {:else}
          <FileWarning aria-hidden="true" class="size-7 text-muted-foreground" />
          <p class="max-w-xs text-sm font-medium">Only Spendo books open here</p>
        {/if}
      </div>
    </div>
  {/if}
</div>
