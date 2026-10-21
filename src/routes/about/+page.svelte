<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { resolve } from '$app/paths';
  import { getVersion } from '@tauri-apps/api/app';
  import { Button } from '$lib/components/ui/button';
  import * as Card from '$lib/components/ui/card';
  import { dbSession } from '$lib/stores/db.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import UpdateCard from '$lib/features/updates/update-card.svelte';

  const meta = PAGE_META.about;

  let appVersion = $state<string | null>(null);

  $effect(() => {
    void getVersion()
      .then((v) => (appVersion = v))
      .catch(() => {
        // Decorative; omit on failure (plain-browser dev).
      });
  });
</script>

<svelte:head>
  <title>About · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<div class="page-stack">
  <PageHeader title={meta.title} description={meta.description}>
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
        Spendo is a local-first finance manager. Your money lives in portable
        <code>.spendo</code> books on this machine: transactions, accounts, budgets, schedules, reports
        and investment prices. GnuCash books and bank CSVs can be imported into the open book.
      </p>
      {#if appVersion}
        <p class="font-mono text-xs text-muted-foreground">Version {appVersion}</p>
      {/if}
    </Card.Content>
  </Card.Root>

  <UpdateCard />
</div>
