<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import * as Popover from '$lib/components/ui/popover';
  import { cn } from '$lib/utils';

  /**
   * Combobox for the Description field: keeps the caller’s text editing the
   * same, but suggests recent descriptions (with the accounts they used) in a
   * popover. Native `<datalist>` was unreliable across Tauri’s webviews and
   * couldn’t show that context. The list opens only once the user types or
   * presses ArrowDown, never on focus, and closes on any click outside.
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

  let inputRef = $state<HTMLInputElement | null>(null);
  let open = $state(false);
  let highlight = $state(-1);

  let query = $derived(value.trim().toLowerCase());
  let filtered = $derived(
    query ? options.filter((o) => o.description.toLowerCase().includes(query)) : options
  );
  // Nothing to suggest means nothing to show: no "no matches" popover.
  let visible = $derived(open && filtered.length > 0);

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

  function handleInput(e: Event & { currentTarget: HTMLInputElement }) {
    highlight = -1;
    open = options.length > 0 && e.currentTarget.value.trim() !== '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (visible) {
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
    if (e.key === 'Enter' && visible) {
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

  // Clicks in the input itself keep the list; any other outside click closes it.
  function handleInteractOutside(e: PointerEvent) {
    if (e.target instanceof Node && inputRef?.contains(e.target)) e.preventDefault();
  }

  function closeOnFocusOut(e: FocusEvent) {
    const next = e.relatedTarget;
    if (next instanceof HTMLElement && next.closest('[data-slot="popover-content"]')) return;
    open = false;
  }

  function pickOnMouseDown(e: MouseEvent) {
    e.preventDefault();
  }

  // Only a real pointer move takes the highlight. WebKit replays hover after
  // layout changes, so a resting pointer would otherwise steal it back from
  // the Arrow keys.
  let lastPointer = { x: -1, y: -1 };
  function hoverOption(e: PointerEvent, i: number) {
    if (e.clientX === lastPointer.x && e.clientY === lastPointer.y) return;
    lastPointer = { x: e.clientX, y: e.clientY };
    highlight = i;
  }

  // Keep the keyboard highlight inside the scrollable list.
  $effect(() => {
    if (!visible || highlight < 0) return;
    document.getElementById(`${listId}-${highlight}`)?.scrollIntoView({ block: 'nearest' });
  });
</script>

<!-- No Popover.Trigger: a trigger would toggle the list on every click in the
     input. The content anchors to the input instead and never traps focus. -->
<Popover.Root open={visible} onOpenChange={(next) => (open = next)}>
  <Input
    bind:ref={inputRef}
    type="text"
    {id}
    bind:value
    {placeholder}
    {maxlength}
    {autofocus}
    autocomplete="off"
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={visible}
    aria-controls={visible ? listId : undefined}
    aria-activedescendant={visible && highlight >= 0 ? `${listId}-${highlight}` : undefined}
    aria-autocomplete="list"
    oninput={handleInput}
    onkeydown={handleKeydown}
    onblur={closeOnFocusOut}
  />
  <Popover.Content
    class="w-(--bits-popover-anchor-width) p-1"
    align="start"
    customAnchor={inputRef}
    trapFocus={false}
    onOpenAutoFocus={keepFocus}
    onCloseAutoFocus={keepFocus}
    onInteractOutside={handleInteractOutside}
  >
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
            onpointermove={(e) => hoverOption(e, i)}
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
  </Popover.Content>
</Popover.Root>
