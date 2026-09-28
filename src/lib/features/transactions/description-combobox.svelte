<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import * as Popover from '$lib/components/ui/popover';
  import { cn } from '$lib/utils';

  /**
   * Combobox for the Description field: keeps the caller’s text editing the
   * same, but suggests recent descriptions (with the accounts they used) in a
   * popover. Native `<datalist>` was unreliable across Tauri’s webviews and
   * couldn’t show that context.
   */
  let {
    value = $bindable(''),
    options = [],
    id = undefined,
    placeholder = '',
    maxlength = undefined,
    autofocus = false,
    onPick
  }: {
    value?: string;
    options?: { description: string; hint?: string }[];
    id?: string;
    placeholder?: string;
    maxlength?: number;
    autofocus?: boolean;
    /** Called after a suggestion is chosen (to restore the remembered splits). */
    onPick?: (description: string) => void;
  } = $props();

  let listId = $derived(`${id ?? 'description'}-options`);

  let open = $state(false);
  let highlight = $state(-1);

  let query = $derived(value.trim().toLowerCase());
  let filtered = $derived(
    query ? options.filter((o) => o.description.toLowerCase().includes(query)) : options
  );

  function show() {
    if (options.length === 0) return;
    highlight = -1;
    open = true;
  }

  function pick(option: { description: string }) {
    value = option.description;
    highlight = -1;
    open = false;
    onPick?.(option.description);
  }

  function handleInput() {
    highlight = -1;
    if (!open && options.length > 0) open = true;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (open) {
        e.preventDefault();
        open = false;
      }
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!open) show();
      const count = filtered.length;
      if (count === 0) return;
      const dir = e.key === 'ArrowDown' ? 1 : -1;
      highlight = (highlight + dir + count) % count;
      return;
    }
    if (e.key === 'Enter' && open) {
      e.preventDefault();
      if (highlight >= 0 && highlight < filtered.length) pick(filtered[highlight]);
      else open = false;
    }
  }

  // Keep the caret in the input while the list is open; it is keyboard-driven
  // from there (Arrow keys and Enter).
  function keepFocus(e: Event) {
    e.preventDefault();
  }

  function closeOnFocusOut(e: FocusEvent) {
    const next = e.relatedTarget;
    if (next instanceof HTMLElement && next.closest('[data-slot="popover-content"]')) return;
    open = false;
  }

  function pickOnMouseDown(e: MouseEvent) {
    e.preventDefault();
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      <Input
        {...props}
        type="text"
        {id}
        bind:value
        {placeholder}
        {maxlength}
        {autofocus}
        autocomplete="off"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-activedescendant={open && highlight >= 0 ? `${listId}-${highlight}` : undefined}
        aria-autocomplete="list"
        oninput={handleInput}
        onfocus={show}
        onkeydown={handleKeydown}
        onblur={closeOnFocusOut}
      />
    {/snippet}
  </Popover.Trigger>
  <Popover.Content
    class="w-(--bits-popover-anchor-width) p-1"
    align="start"
    onOpenAutoFocus={keepFocus}
  >
    {#if filtered.length > 0}
      <ul id={listId} class="flex max-h-64 flex-col overflow-y-auto" role="listbox">
        {#each filtered as option, i (option.description)}
          <li>
            <button
              id="{listId}-{i}"
              type="button"
              role="option"
              aria-selected={highlight === i}
              tabindex="-1"
              class={cn(
                'flex w-full cursor-default flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none',
                highlight === i && 'bg-accent text-accent-foreground'
              )}
              onmousedown={pickOnMouseDown}
              onclick={() => pick(option)}
              onmouseenter={() => (highlight = i)}
            >
              <span class="w-full truncate">{option.description}</span>
              {#if option.hint}
                <span
                  class="w-full truncate text-xs {highlight === i
                    ? 'text-accent-foreground/80'
                    : 'text-muted-foreground'}"
                >
                  {option.hint}
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="px-2 py-1.5 text-sm text-muted-foreground">
        {query ? 'No recent description matches that.' : 'No recent descriptions yet.'}
      </p>
    {/if}
  </Popover.Content>
</Popover.Root>
