<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import {
    Wallet,
    Database,
    TrendingUp,
    Calendar,
    Coins,
    LayoutDashboard,
    LogOut,
    ChevronRight
  } from '@lucide/svelte';
  import { dbSession, clearSession } from '$lib/stores/db.svelte';
  import UploadDialog from '$lib/components/upload-dialog.svelte';
  import { formatNumber, formatDate as fmtDate } from '$lib/format.svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { fade } from 'svelte/transition';
</script>

<svelte:head>
  <title>Spendo</title>
  <meta name="description" content="Spendo — expense tracking app" />
</svelte:head>

<section class="flex flex-col gap-10">
  {#if !dbSession.value}
    <div class="flex flex-col items-center gap-6 text-center">
      <div class="flex size-20 items-center justify-center rounded-2xl bg-primary/8">
        <Wallet aria-hidden="true" class="size-10 text-primary" />
      </div>
      <div class="flex flex-col gap-2">
        <h1 class="text-4xl font-bold tracking-tight sm:text-5xl">Spendo</h1>
        <p class="text-lg text-muted-foreground">
          Upload your GnuCash file and explore your financial data.
        </p>
      </div>
    </div>

    <div in:fade={{ duration: 300 }}>
      <UploadDialog />
    </div>

    <div class="grid grid-cols-1 gap-6 sm:grid-cols-3">
      <div class="flex flex-col items-center gap-2 text-center">
        <div class="flex size-10 items-center justify-center rounded-lg bg-primary/8">
          <TrendingUp aria-hidden="true" class="size-5 text-primary" />
        </div>
        <div class="flex flex-col gap-0.5">
          <p class="text-sm font-medium">Income &amp; Expense Tracking</p>
          <p class="text-xs text-muted-foreground">
            Monitor where your money comes from and where it goes
          </p>
        </div>
      </div>
      <div class="flex flex-col items-center gap-2 text-center">
        <div class="flex size-10 items-center justify-center rounded-lg bg-primary/8">
          <LayoutDashboard aria-hidden="true" class="size-5 text-primary" />
        </div>
        <div class="flex flex-col gap-0.5">
          <p class="text-sm font-medium">Visual Dashboards</p>
          <p class="text-xs text-muted-foreground">Interactive charts and spending breakdowns</p>
        </div>
      </div>
      <div class="flex flex-col items-center gap-2 text-center">
        <div class="flex size-10 items-center justify-center rounded-lg bg-primary/8">
          <Coins aria-hidden="true" class="size-5 text-primary" />
        </div>
        <div class="flex flex-col gap-0.5">
          <p class="text-sm font-medium">Multi-Currency Support</p>
          <p class="text-xs text-muted-foreground">Works with any currency in your GnuCash file</p>
        </div>
      </div>
    </div>
  {:else}
    {@const s = dbSession.value.summary}

    <!-- Compact header bar -->
    <div class="flex items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <h2 class="text-3xl font-bold tracking-tight">Database</h2>
        <p class="text-sm text-muted-foreground">
          {dbSession.value.filename ?? 'Database'}
        </p>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        <Button onclick={() => goto(resolve('/dashboard'))}>
          <LayoutDashboard aria-hidden="true" class="mr-1.5 size-4" />
          Dashboard
        </Button>
        <Button variant="destructive" onclick={clearSession} aria-label="Close database">
          <LogOut aria-hidden="true" class="size-4" />
        </Button>
      </div>
    </div>

    <!-- Stat cards + info chips -->
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2" in:fade={{ duration: 300 }}>
      <!-- Accounts — clickable -->
      <Card.Root class="cursor-pointer group" onclick={() => goto(resolve('/accounts'))}>
        <Card.Header>
          <div class="flex items-center gap-2">
            <Database aria-hidden="true" class="size-4 text-muted-foreground" />
            <Card.Title>Accounts</Card.Title>
          </div>
          <Card.Action>
            <ChevronRight
              aria-hidden="true"
              class="size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5"
            />
          </Card.Action>
        </Card.Header>
        <Card.Content class="text-3xl font-bold tracking-tight">
          {formatNumber(s.accountsCount)}
        </Card.Content>
      </Card.Root>

      <!-- Transactions — clickable -->
      <Card.Root class="cursor-pointer group" onclick={() => goto(resolve('/transactions'))}>
        <Card.Header>
          <div class="flex items-center gap-2">
            <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
            <Card.Title>Transactions</Card.Title>
          </div>
          <Card.Action>
            <ChevronRight
              aria-hidden="true"
              class="size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5"
            />
          </Card.Action>
        </Card.Header>
        <Card.Content class="text-3xl font-bold tracking-tight">
          {formatNumber(s.transactionsCount)}
        </Card.Content>
      </Card.Root>

      <!-- Currency — info chip -->
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <Coins aria-hidden="true" class="size-4 text-muted-foreground" />
            <Card.Title>Currency</Card.Title>
          </div>
        </Card.Header>
        <Card.Content>
          <p class="text-3xl font-bold tracking-tight">{s.currency}</p>
        </Card.Content>
      </Card.Root>

      <!-- Date Range — info chip -->
      <Card.Root>
        <Card.Header>
          <div class="flex items-center gap-2">
            <Calendar aria-hidden="true" class="size-4 text-muted-foreground" />
            <Card.Title>Date range</Card.Title>
          </div>
        </Card.Header>
        <Card.Content class="text-3xl font-bold tracking-tight">
          {#if s.dateRange}
            {fmtDate(s.dateRange.from)} &ndash; {fmtDate(s.dateRange.to)}
          {:else}
            No transactions
          {/if}
        </Card.Content>
      </Card.Root>
    </div>
  {/if}
</section>
