<script lang="ts">
  import { resolve } from '$app/paths';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import { Wallet, Terminal, Palette, Component, Paintbrush, CloudDownload } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
</script>

<svelte:head>
  <title>About · Spendo</title>
  <meta name="description" content="About Spendo" />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader title="About Spendo" description="The short version">
    {#if !dbSession.value}
      <Button variant="outline" href={resolve('/')}>Welcome</Button>
    {/if}
  </PageHeader>

  <Card.Root>
    <Card.Header>
      <Card.Title>What is Spendo?</Card.Title>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <p class="text-sm text-muted-foreground">
        Spendo keeps your money organised in portable
        <code>.spendo</code> books. It runs natively via Tauri with a static SvelteKit frontend and a
        Rust/SQLite backend.
      </p>
      <p class="text-sm text-muted-foreground">
        Open a book to browse accounts, dig into transactions, track investment prices, and view
        interactive dashboards. Coming from GnuCash? Import your
        <code>.gnucash</code> file into the open book. All data stays local on your machine.
      </p>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <Card.Title>Tech Stack</Card.Title>
    </Card.Header>
    <Card.Content>
      <ul class="flex flex-col gap-3">
        <li class="flex items-center gap-3">
          <Wallet aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">Tauri v2</span>
            <Badge variant="secondary">Desktop Shell</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <Terminal aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">Rust + SQLite (rusqlite)</span>
            <Badge variant="secondary">Backend</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <Palette aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">Svelte 5 / SvelteKit (adapter-static)</span>
            <Badge variant="secondary">Frontend</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <Component aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">shadcn-svelte (Nova)</span>
            <Badge variant="secondary">UI Components</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <Paintbrush aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">Tailwind CSS v4</span>
            <Badge variant="secondary">Styling</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <CloudDownload aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">Yahoo Finance API (reqwest)</span>
            <Badge variant="secondary">Live Prices</Badge>
          </div>
        </li>
      </ul>
    </Card.Content>
  </Card.Root>
</div>
