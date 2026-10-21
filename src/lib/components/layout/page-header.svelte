<script lang="ts" module>
  /** One ancestor link above the page title; the title itself is the current page. */
  export interface Breadcrumb {
    label: string;
    href?: string;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronRight } from '@lucide/svelte';

  let {
    title,
    description,
    breadcrumbs = [],
    children
  }: {
    title: string;
    description?: string | Snippet;
    breadcrumbs?: Breadcrumb[];
    children?: Snippet;
  } = $props();
</script>

<div class="flex flex-col gap-2">
  {#if breadcrumbs.length > 0}
    <nav aria-label="Breadcrumb">
      <ol class="flex min-w-0 flex-wrap items-center gap-1 text-xs text-muted-foreground">
        {#each breadcrumbs as crumb, i (i)}
          <li class="flex min-w-0 items-center gap-1">
            {#if crumb.href}
              <!-- eslint-disable svelte/no-navigation-without-resolve -- callers pass resolved hrefs -->
              <a
                href={crumb.href}
                class="truncate rounded-sm hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >{crumb.label}</a
              >
              <!-- eslint-enable svelte/no-navigation-without-resolve -->
            {:else}
              <span class="truncate">{crumb.label}</span>
            {/if}
            <ChevronRight aria-hidden="true" class="size-3 shrink-0" />
          </li>
        {/each}
      </ol>
    </nav>
  {/if}
  <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-3">
    <div class="flex min-w-0 shrink grow basis-80 flex-col gap-1">
      <h1 class="text-2xl font-semibold tracking-tight break-words">{title}</h1>
      {#if description !== undefined}
        {#if typeof description === 'function'}
          <!-- min-h-5 reserves the text-sm line height so an empty description
               (e.g. before data loads) doesn’t collapse and shift the page. -->
          <p class="min-h-5 text-sm text-muted-foreground">{@render description()}</p>
        {:else}
          <p class="min-h-5 text-sm text-muted-foreground">{description}</p>
        {/if}
      {/if}
    </div>
    {#if children}
      <div class="ml-auto flex shrink-0 flex-wrap items-center gap-2">
        {@render children()}
      </div>
    {/if}
  </div>
</div>
