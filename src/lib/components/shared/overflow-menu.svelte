<script lang="ts" module>
  import type { Component } from 'svelte';

  /** One secondary page action. `href` must already be resolved. */
  export interface OverflowAction {
    label: string;
    icon: Component;
    onSelect?: () => void;
    href?: string;
    disabled?: boolean;
    destructive?: boolean;
  }
</script>

<script lang="ts">
  import { Ellipsis } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';

  /**
   * "More actions" button for page headers: one primary action stays visible,
   * the secondary ones live here so headers don’t grow a row of buttons.
   */
  let { actions, label = 'More actions' }: { actions: OverflowAction[]; label?: string } = $props();
</script>

{#if actions.length > 0}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" size="icon" aria-label={label}>
          <Ellipsis aria-hidden="true" class="size-4" />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content align="end" class="min-w-44">
      {#each actions as action (action.label)}
        {#if action.href}
          <DropdownMenu.Item disabled={action.disabled}>
            {#snippet child({ props })}
              <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- callers pass resolved hrefs -->
              <a href={action.href} {...props}>
                <action.icon aria-hidden="true" />
                {action.label}
              </a>
            {/snippet}
          </DropdownMenu.Item>
        {:else}
          {#if action.destructive}
            <DropdownMenu.Separator />
          {/if}
          <DropdownMenu.Item
            disabled={action.disabled}
            variant={action.destructive ? 'destructive' : 'default'}
            onSelect={action.onSelect}
          >
            <action.icon aria-hidden="true" />
            {action.label}
          </DropdownMenu.Item>
        {/if}
      {/each}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
{/if}
