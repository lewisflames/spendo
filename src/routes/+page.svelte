<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import {
    Wallet,
    Upload,
    LoaderCircle,
    LayoutDashboard,
    Database,
    ArrowLeftRight,
    TrendingUp,
    AlertCircle
  } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchHomeData, uploadDb } from '$lib/db.remote';
  import { resolve } from '$app/paths';
  import ImportHistory from '$lib/components/ImportHistory.svelte';

  // --- inline import ---
  let importForm = $state<HTMLFormElement | null>(null);
  let importInput = $state<HTMLInputElement | null>(null);
  let importing = $state(false);
  let importError = $state<string | null>(null);

  $effect(() => {
    let cancelled = false;
    fetchHomeData({}).then((result) => {
      if (cancelled) return;
      if (result.hasData && result.summary) {
        dbSession.value = { summary: result.summary, filename: null };
      }
    });
    return () => {
      cancelled = true;
    };
  });

  function handleImportFile() {
    if (!importForm || !importInput?.files?.[0]) return;
    importError = null;
    importing = true;
    importForm.requestSubmit();
  }

  $effect(() => {
    const r = uploadDb.result;
    if (!r || !importing) return;
    if (r.ok) {
      dbSession.value = {
        summary: r.importResult.summary,
        filename: importInput?.files?.[0]?.name ?? null
      };
      if (importInput) importInput.value = '';
    } else {
      importError = r.error;
    }
    importing = false;
  });
</script>

<svelte:head>
  <title>Spendo</title>
  <meta name="description" content="Spendo — expense tracking app" />
</svelte:head>

<section class="mt-16 flex flex-col gap-10">
  <!-- Hero -->
  <div class="flex flex-col items-center gap-6 text-center">
    <div class="flex size-20 items-center justify-center rounded-2xl bg-primary/8">
      <Wallet aria-hidden="true" class="size-10 text-primary" />
    </div>
    <div class="flex flex-col gap-2">
      <h1 class="text-4xl font-bold tracking-tight sm:text-5xl">Spendo</h1>
      <p class="text-lg text-muted-foreground">
        Import your GnuCash file and start exploring your finances.
      </p>
    </div>
  </div>

  <!-- C: Quick-navigation cards -->
  {#if dbSession.value}
    <div class="mx-auto grid w-full max-w-4xl grid-cols-2 gap-4 lg:grid-cols-4">
      <a
        href={resolve('/dashboard')}
        class="rounded-xl transition-colors hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        <Card.Root size="sm" class="cursor-pointer">
          <Card.Content class="flex items-center gap-3">
            <div class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/8">
              <LayoutDashboard aria-hidden="true" class="size-4 text-primary" />
            </div>
            <div class="flex min-w-0 flex-col gap-0.5">
              <Card.Title class="text-sm">Dashboard</Card.Title>
              <Card.Description>Financial overview</Card.Description>
            </div>
          </Card.Content>
        </Card.Root>
      </a>
      <a
        href={resolve('/accounts')}
        class="rounded-xl transition-colors hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        <Card.Root size="sm" class="cursor-pointer">
          <Card.Content class="flex items-center gap-3">
            <div class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/8">
              <Database aria-hidden="true" class="size-4 text-primary" />
            </div>
            <div class="flex min-w-0 flex-col gap-0.5">
              <Card.Title class="text-sm">Accounts</Card.Title>
              <Card.Description>Chart of accounts</Card.Description>
            </div>
          </Card.Content>
        </Card.Root>
      </a>
      <a
        href={resolve('/transactions')}
        class="rounded-xl transition-colors hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        <Card.Root size="sm" class="cursor-pointer">
          <Card.Content class="flex items-center gap-3">
            <div class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/8">
              <ArrowLeftRight aria-hidden="true" class="size-4 text-primary" />
            </div>
            <div class="flex min-w-0 flex-col gap-0.5">
              <Card.Title class="text-sm">Transactions</Card.Title>
              <Card.Description>All transactions</Card.Description>
            </div>
          </Card.Content>
        </Card.Root>
      </a>
      <a
        href={resolve('/prices')}
        class="rounded-xl transition-colors hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      >
        <Card.Root size="sm" class="cursor-pointer">
          <Card.Content class="flex items-center gap-3">
            <div class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/8">
              <TrendingUp aria-hidden="true" class="size-4 text-primary" />
            </div>
            <div class="flex min-w-0 flex-col gap-0.5">
              <Card.Title class="text-sm">Prices</Card.Title>
              <Card.Description>Commodity prices</Card.Description>
            </div>
          </Card.Content>
        </Card.Root>
      </a>
    </div>
  {/if}

  <!-- Import History -->
  <div class="mx-auto flex w-full max-w-4xl flex-col gap-4">
    <form bind:this={importForm} {...uploadDb} enctype="multipart/form-data" class="hidden">
      <input
        bind:this={importInput}
        type="file"
        name="dbFile"
        accept=".gnucash"
        onchange={handleImportFile}
        aria-label="Choose a GnuCash file to import"
      />
    </form>
    <div class="flex items-center justify-between">
      <h3 class="text-lg font-semibold tracking-tight">Import History</h3>
      <div class="flex items-center gap-2">
        {#if importing}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin text-primary" />
        {:else}
          <Button variant="outline" onclick={() => importInput?.click()}>
            <Upload aria-hidden="true" class="size-3" />
            Import
          </Button>
        {/if}
      </div>
    </div>

    <!-- F: Error banner -->
    {#if importError}
      <Card.Root class="border-destructive/50" size="sm">
        <Card.Content class="flex items-center gap-2 py-3 text-sm text-destructive">
          <AlertCircle aria-hidden="true" class="size-4 shrink-0" />
          {importError}
        </Card.Content>
      </Card.Root>
    {/if}

    <ImportHistory />
  </div>
</section>
