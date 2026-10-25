<script lang="ts">
  import icon from '$icon';
  import { REPO_URL, RELEASES_URL } from '$lib/release';
  import Screenshot from '$lib/screenshot.svelte';
  import { SCREENSHOTS } from '$lib/screenshots';
  import {
    ArrowDownToLine,
    ChartPie,
    FileInput,
    HardDrive,
    History,
    ListChecks,
    TrendingUp
  } from '@lucide/svelte';

  let { data } = $props();
  const release = $derived(data.release);

  const released = $derived(
    release.publishedAt
      ? new Date(release.publishedAt).toLocaleDateString('en', { month: 'long', year: 'numeric' })
      : null
  );

  const title = 'Spendo · Local-first personal finance';
  const description =
    'A personal finance app for macOS and Linux. Your money lives in a single .spendo file on your own disk: no account, no cloud.';

  const tour = [
    {
      shot: SCREENSHOTS.transactions,
      title: 'Transactions',
      body: 'Search everything, filter by payee or tag, and edit in bulk.'
    },
    {
      shot: SCREENSHOTS.spending,
      title: 'Spending',
      body: 'Where the money went each month, drilling down into any category.'
    },
    {
      shot: SCREENSHOTS.budgets,
      title: 'Budgets',
      body: 'Yearly budgets that show whether you are ahead or behind for the date.'
    },
    {
      shot: SCREENSHOTS.portfolio,
      title: 'Portfolio',
      body: 'What your investments are worth against what you paid, with returns and dividends.'
    }
  ];

  const features = [
    {
      icon: HardDrive,
      title: 'One file, on your disk',
      body: 'A book is a single .spendo file. Open it, move it, copy it or keep it in your own sync folder, like any other document.'
    },
    {
      icon: ChartPie,
      title: 'Budgets and reports',
      body: 'Yearly budgets, profit and loss, a balance sheet with comparisons, a spending breakdown and a yearly tax summary.'
    },
    {
      icon: TrendingUp,
      title: 'Investments',
      body: 'Holdings priced from Yahoo Finance, FIFO or average cost, realized gains, dividends and money-weighted returns.'
    },
    {
      icon: ListChecks,
      title: 'Reconcile and schedule',
      body: 'Match statements and undo the last one if you slip. Scheduled transactions can post themselves when they fall due.'
    },
    {
      icon: FileInput,
      title: 'Bring your history',
      body: 'Merge a GnuCash book or import bank CSV statements, always previewed before anything is written. Rules pick categories.'
    },
    {
      icon: History,
      title: 'Backups built in',
      body: 'Daily snapshots, plus a safety snapshot before every import, bulk edit or reset. Restore one from Settings.'
    }
  ];
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta property="og:type" content="website" />
</svelte:head>

<header class="mx-auto flex max-w-5xl items-center justify-between px-4 py-5 sm:px-6">
  <a href="#top" class="flex items-center gap-2.5 font-semibold tracking-tight">
    <img src={icon} alt="" class="size-8" />
    Spendo
  </a>
  <nav class="flex items-center gap-5 text-sm text-muted-foreground">
    <a href="#features" class="hidden hover:text-foreground sm:inline">Features</a>
    <a href="#install" class="hover:text-foreground">Install</a>
    <a href={REPO_URL} class="hover:text-foreground">GitHub</a>
  </nav>
</header>

<main id="top">
  <section class="mx-auto max-w-5xl px-4 pt-12 sm:px-6 md:pt-20">
    <div>
      <p class="mb-4 text-sm font-medium text-brand">For macOS and Linux · Free and open source</p>
      <h1 class="text-4xl font-semibold tracking-tight text-balance sm:text-5xl">
        Your money, in a file you own.
      </h1>
      <p class="mt-5 max-w-xl text-lg text-pretty text-muted-foreground">
        Spendo is a local-first personal finance app. Accounts, budgets, reports and investments
        live in a single <code class="font-mono text-foreground">.spendo</code> file on your disk. No
        account, no cloud.
      </p>
      <div class="mt-8 flex flex-wrap gap-3">
        <a
          href={release.mac}
          class="inline-flex items-center gap-2 rounded-lg bg-brand px-4 py-2.5 text-sm font-medium text-brand-foreground shadow-sm hover:opacity-90"
        >
          <ArrowDownToLine class="size-4" />
          Download for macOS
        </a>
        <a
          href={release.appImage}
          class="inline-flex items-center gap-2 rounded-lg border bg-card px-4 py-2.5 text-sm font-medium hover:bg-muted"
        >
          <ArrowDownToLine class="size-4" />
          Download for Linux
        </a>
      </div>
      <p class="mt-4 text-sm text-muted-foreground">
        {#if release.version}
          Version {release.version}{released ? `, released ${released}` : ''} ·
        {/if}
        <a href={RELEASES_URL} class="underline underline-offset-4 hover:text-foreground"
          >All downloads</a
        >
      </p>
    </div>
  </section>

  <div class="relative mx-auto max-w-6xl px-4 pt-14 pb-20 sm:px-6">
    <div
      class="absolute inset-x-10 top-24 bottom-10 -z-10 rounded-full bg-brand/15 blur-3xl"
      aria-hidden="true"
    ></div>
    <Screenshot shot={SCREENSHOTS.overview} eager />
  </div>

  <section id="features" class="border-t bg-card">
    <div class="mx-auto max-w-5xl px-4 py-20 sm:px-6">
      <h2 class="text-2xl font-semibold tracking-tight">Everything a household ledger needs</h2>
      <p class="mt-2 max-w-2xl text-muted-foreground">
        Double-entry accounts with splits underneath, plain screens on top.
      </p>
      <div class="mt-10 grid gap-x-8 gap-y-10 sm:grid-cols-2 lg:grid-cols-3">
        {#each features as feature (feature.title)}
          {@const Icon = feature.icon}
          <div>
            <div
              class="flex size-9 items-center justify-center rounded-lg bg-brand-soft text-brand"
            >
              <Icon class="size-5" />
            </div>
            <h3 class="mt-4 font-medium">{feature.title}</h3>
            <p class="mt-1.5 text-sm/6 text-muted-foreground">{feature.body}</p>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <section id="tour" class="border-t">
    <div class="mx-auto max-w-6xl px-4 py-20 sm:px-6">
      <h2 class="text-2xl font-semibold tracking-tight">A closer look</h2>
      <p class="mt-2 max-w-2xl text-muted-foreground">
        A made-up household book, as it appears in the app.
      </p>
      <div class="mt-10 grid gap-x-8 gap-y-12 md:grid-cols-2">
        {#each tour as item (item.title)}
          <figure>
            <Screenshot shot={item.shot} />
            <figcaption class="mt-4">
              <span class="font-medium">{item.title}.</span>
              <span class="text-muted-foreground">{item.body}</span>
            </figcaption>
          </figure>
        {/each}
      </div>
    </div>
  </section>

  <section class="border-t bg-card">
    <div class="mx-auto grid max-w-5xl gap-8 px-4 py-20 sm:px-6 md:grid-cols-2">
      <div>
        <h2 class="text-2xl font-semibold tracking-tight">Private by design</h2>
        <p class="mt-3 text-muted-foreground">
          There is no Spendo server and nothing to sign up for. The app only goes online to fetch
          security prices from Yahoo Finance and to check GitHub for updates.
        </p>
      </div>
      <ul class="space-y-3 text-sm">
        <li class="rounded-lg border bg-card p-4">
          <span class="font-medium">Yours to keep.</span>
          <span class="text-muted-foreground">
            A book is a standard SQLite database, so your data never depends on the app.</span
          >
        </li>
        <li class="rounded-lg border bg-card p-4">
          <span class="font-medium">Coming from GnuCash?</span>
          <span class="text-muted-foreground">
            Merge a <code class="font-mono">.gnucash</code> SQLite book as often as you like. The source
            is only ever read.</span
          >
        </li>
      </ul>
    </div>
  </section>

  <section id="install" class="border-t">
    <div class="mx-auto max-w-5xl px-4 py-20 sm:px-6">
      <h2 class="text-2xl font-semibold tracking-tight">Install</h2>
      <div class="mt-8 grid gap-4 md:grid-cols-3">
        <div class="rounded-xl border bg-card p-5">
          <h3 class="font-medium">macOS</h3>
          <p class="mt-2 text-sm/6 text-muted-foreground">
            Download the <a href={release.mac} class="text-brand hover:underline">universal dmg</a>.
            The app is not notarized, so on first launch right-click it and choose
            <strong class="font-medium text-foreground">Open</strong>. It updates itself from About
            → Updates.
          </p>
        </div>
        <div class="rounded-xl border bg-card p-5">
          <h3 class="font-medium">Linux</h3>
          <p class="mt-2 text-sm/6 text-muted-foreground">
            Download the <a href={release.appImage} class="text-brand hover:underline">AppImage</a>,
            make it executable and run it. It updates itself too.
          </p>
          <pre class="mt-3 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs"><code
              >chmod +x Spendo_*.AppImage</code
            ></pre>
          <p class="mt-3 text-sm text-muted-foreground">
            Prefer a package? There's a <a href={release.deb} class="text-brand hover:underline"
              >deb</a
            >.
          </p>
        </div>
        <div class="rounded-xl border bg-card p-5">
          <h3 class="font-medium">Arch Linux</h3>
          <p class="mt-2 text-sm/6 text-muted-foreground">
            Install <a
              href="https://aur.archlinux.org/packages/spendo-bin"
              class="text-brand hover:underline">spendo-bin</a
            > from the AUR and update it with your AUR helper.
          </p>
          <pre class="mt-3 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs"><code
              >yay -S spendo-bin</code
            ></pre>
        </div>
      </div>
    </div>
  </section>
</main>

<footer class="border-t">
  <div
    class="mx-auto flex max-w-5xl flex-col gap-2 px-4 py-8 text-sm text-muted-foreground sm:flex-row sm:justify-between sm:px-6"
  >
    <p>Spendo is MIT licensed.</p>
    <div class="flex gap-5">
      <a href={REPO_URL} class="hover:text-foreground">Source</a>
      <a href={RELEASES_URL} class="hover:text-foreground">Releases</a>
      <a href="{REPO_URL}/issues" class="hover:text-foreground">Report an issue</a>
    </div>
  </div>
</footer>
