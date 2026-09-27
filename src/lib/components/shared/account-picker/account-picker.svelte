<script lang="ts">
  import * as Popover from '$lib/components/ui/popover';
  import * as Command from '$lib/components/ui/command';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import { Check, ChevronsUpDown } from '@lucide/svelte';
  import { cn } from '$lib/utils.js';
  import type { PickerAccount } from '$lib/features/accounts/account-picker-utils';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';

  let {
    value = $bindable(''),
    options = [],
    placeholder = 'Pick an account',
    id,
    disabled = false,
    invalid = false,
    describedBy = undefined,
    emptyText = 'Nothing matches that search.',
    noneLabel = null
  }: {
    value?: string;
    options?: PickerAccount[];
    placeholder?: string;
    id?: string;
    disabled?: boolean;
    invalid?: boolean;
    describedBy?: string | undefined;
    emptyText?: string;
    /** When set, offers an item that clears the selection (e.g. "Top level"). */
    noneLabel?: string | null;
  } = $props();

  let open = $state(false);
  let search = $state('');

  let selected = $derived(options.find((o) => o.guid === value) ?? null);

  // Fresh query on every open so a previous search never leaks through.
  $effect(() => {
    if (!open) search = '';
  });

  function select(guid: string) {
    value = guid;
    open = false;
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger {id} {disabled} aria-invalid={invalid} aria-describedby={describedBy}>
    {#snippet child({ props })}
      <Button
        {...props}
        type="button"
        variant="outline"
        role="combobox"
        aria-expanded={open}
        {disabled}
        title={selected?.label}
        class={cn('w-full justify-between font-normal', !selected && 'text-muted-foreground')}
      >
        <span class="min-w-0 flex-1 truncate text-left">
          {selected ? selected.display : placeholder}
          {#if selected?.parentPath}
            <span class="text-muted-foreground">· {selected.parentPath}</span>
          {/if}
        </span>
        {#if selected}
          <Badge
            variant="outline"
            class="ml-2 hidden shrink-0 text-[10px] leading-none sm:inline-flex"
          >
            {selected.accountType}
          </Badge>
        {/if}
        <ChevronsUpDown aria-hidden="true" class="ml-2 size-4 shrink-0 opacity-50" />
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content class="w-(--bits-popover-anchor-width) p-0" align="start">
    <Command.Root label="Select an account">
      <Command.Input bind:value={search} placeholder="Search accounts…" />
      <Command.List>
        <Command.Empty>{emptyText}</Command.Empty>
        <Command.Group>
          {#if noneLabel !== null}
            <Command.Item value="__none__" keywords={[noneLabel]} onSelect={() => select('')}>
              <Check
                aria-hidden="true"
                class={cn('mr-1 size-4', value === '' ? 'opacity-100' : 'opacity-0')}
              />
              <span class="min-w-0 flex-1 truncate">{noneLabel}</span>
            </Command.Item>
          {/if}
          {#each options as opt (opt.guid)}
            <Command.Item
              value={opt.guid}
              keywords={[
                opt.label,
                opt.display,
                opt.description ?? '',
                opt.code ?? '',
                opt.accountType,
                opt.commodity ?? ''
              ]}
              title={opt.label}
              onSelect={() => select(opt.guid)}
            >
              <Check
                aria-hidden="true"
                class={cn('mr-1 size-4', value === opt.guid ? 'opacity-100' : 'opacity-0')}
              />
              <span class="flex min-w-0 flex-1 items-start gap-1">
                <AccountLabel name={opt.display} description={opt.description} code={opt.code} />
                {#if opt.parentPath}
                  <span class="min-w-0 truncate pt-0.5 text-xs text-muted-foreground">
                    · {opt.parentPath}
                  </span>
                {/if}
              </span>
              <Badge variant="outline" class="shrink-0 text-[10px] leading-none">
                {opt.accountType}
              </Badge>
            </Command.Item>
          {/each}
        </Command.Group>
      </Command.List>
    </Command.Root>
  </Popover.Content>
</Popover.Root>
