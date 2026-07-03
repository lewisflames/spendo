<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import * as Card from '$lib/components/ui/card';
  import { Palette, Cloud, Terminal } from '@lucide/svelte';
</script>

<svelte:head>
  <title>About Spendo</title>
  <meta name="description" content="About Spendo" />
</svelte:head>

<section class="flex flex-col gap-6">
  <h1 class="text-3xl font-bold tracking-tight">About Spendo</h1>

  <Card.Root>
    <Card.Header>
      <Card.Title>What is Spendo?</Card.Title>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <p class="text-sm text-muted-foreground">
        Spendo is a demo expense-tracking application built to showcase modern Svelte ecosystem
        tooling. It demonstrates a full-stack architecture with server-side rendering, SQLite
        persistence, and a component-driven UI.
      </p>
      <p class="text-sm text-muted-foreground">
        This project was scaffolded with the SvelteKit CLI and uses shadcn-svelte for its design
        system, and Tailwind CSS v4 for styling.
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
          <Terminal aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">SvelteKit</span>
            <Badge variant="secondary">Framework</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <Palette aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">shadcn-svelte</span>
            <Badge variant="secondary">UI Components</Badge>
          </div>
        </li>
        <li class="flex items-center gap-3">
          <Cloud aria-hidden="true" class="size-5 shrink-0 text-primary" />
          <div class="flex flex-1 items-center justify-between gap-2">
            <span class="text-sm font-medium">Tailwind CSS v4</span>
            <Badge variant="secondary">Styling</Badge>
          </div>
        </li>
      </ul>
    </Card.Content>
  </Card.Root>
</section>
