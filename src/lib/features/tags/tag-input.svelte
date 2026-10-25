<script lang="ts">
  import * as Popover from '$lib/components/ui/popover';
  import { X } from '@lucide/svelte';
  import { cn } from '$lib/utils';
  import { addTagName, MAX_TAG_LEN, tagSuggestions } from './tag-utils';

  /**
   * Chip input for transaction tags. Enter, comma or Tab commits the typed
   * name (or the highlighted suggestion), Backspace on an empty field removes
   * the last chip, and leaving the field commits what's typed. Suggestions are
   * the book's tags, most used first, in the same anchored popover as the
   * description combobox.
   */
  let {
    value = $bindable<string[]>([]),
    known = [],
    id = 'tags',
    placeholder = 'Add a tag…',
    describedBy = undefined
  }: {
    value?: string[];
    /** Every tag in the book, most used first. */
    known?: string[];
    id?: string;
    placeholder?: string;
    describedBy?: string;
  } = $props();

  let listId = $derived(`${id}-options`);
  let errorId = $derived(`${id}-error`);

  let inputRef = $state<HTMLInputElement | null>(null);
  let boxRef = $state<HTMLDivElement | null>(null);
  let draft = $state('');
  let open = $state(false);
  let highlight = $state(-1);
  let error = $state('');

  let suggestions = $derived(tagSuggestions(known, value, draft));
  let visible = $derived(open && suggestions.length > 0);

  function commit(raw: string): boolean {
    const result = addTagName(value, raw, known);
    error = result.error ?? '';
    if (result.error) return false;
    value = result.tags;
    draft = '';
    highlight = -1;
    return true;
  }

  function remove(tag: string) {
    value = value.filter((t) => t !== tag);
    error = '';
    inputRef?.focus();
  }

  function handleInput() {
    error = '';
    highlight = -1;
    // A pasted "a, b" commits every complete name before the last comma.
    if (draft.includes(',')) {
      const parts = draft.split(',');
      const rest = parts.pop() ?? '';
      for (const part of parts) if (part.trim() && !commit(part)) return;
      draft = rest;
    }
    open = true;
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
      open = true;
      const count = suggestions.length;
      if (count === 0) return;
      const dir = e.key === 'ArrowDown' ? 1 : -1;
      highlight = (highlight + dir + count) % count;
      return;
    }
    if (e.key === 'Backspace' && draft === '' && value.length > 0) {
      e.preventDefault();
      value = value.slice(0, -1);
      return;
    }
    const picked = visible && highlight >= 0 ? suggestions[highlight] : null;
    if (e.key === 'Enter' || e.key === ',') {
      // Never submit the surrounding form from here.
      if (picked || draft.trim() || e.key === ',') e.preventDefault();
      if (picked) commit(picked);
      else if (draft.trim()) commit(draft);
      return;
    }
    if (e.key === 'Tab' && !e.shiftKey && (picked || draft.trim())) {
      if (commit(picked ?? draft)) e.preventDefault();
    }
  }

  function handleBlur(e: FocusEvent) {
    const next = e.relatedTarget;
    if (next instanceof HTMLElement && next.closest('[data-slot="popover-content"]')) return;
    open = false;
    if (draft.trim()) commit(draft);
  }

  function keepFocus(e: Event) {
    e.preventDefault();
  }

  function handleInteractOutside(e: PointerEvent) {
    if (e.target instanceof Node && boxRef?.contains(e.target)) e.preventDefault();
  }

  let lastPointer = { x: -1, y: -1 };
  function hoverOption(e: PointerEvent, i: number) {
    if (e.clientX === lastPointer.x && e.clientY === lastPointer.y) return;
    lastPointer = { x: e.clientX, y: e.clientY };
    highlight = i;
  }

  $effect(() => {
    if (!visible || highlight < 0) return;
    document.getElementById(`${listId}-${highlight}`)?.scrollIntoView({ block: 'nearest' });
  });
</script>

<Popover.Root open={visible} onOpenChange={(next) => (open = next)}>
  <!-- Clicking the box's padding focuses the field, like a native input. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    bind:this={boxRef}
    class={cn(
      'flex min-h-8 w-full flex-wrap items-center gap-1 rounded-lg border border-input bg-transparent px-1.5 py-1 transition-colors focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50 dark:bg-input/30',
      error && 'border-destructive ring-3 ring-destructive/20'
    )}
    onclick={(e) => {
      if (e.target === e.currentTarget) inputRef?.focus();
    }}
  >
    {#each value as tag (tag)}
      <span
        class="inline-flex h-6 max-w-full items-center gap-0.5 rounded-md bg-secondary pr-0.5 pl-2 text-xs font-medium text-secondary-foreground"
      >
        <span class="truncate">{tag}</span>
        <button
          type="button"
          class="inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-background/60 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
          aria-label="Remove tag {tag}"
          onclick={() => remove(tag)}
        >
          <X aria-hidden="true" class="size-3" />
        </button>
      </span>
    {/each}
    <input
      bind:this={inputRef}
      {id}
      type="text"
      bind:value={draft}
      placeholder={value.length === 0 ? placeholder : ''}
      maxlength={MAX_TAG_LEN + 10}
      autocomplete="off"
      role="combobox"
      aria-haspopup="listbox"
      aria-expanded={visible}
      aria-controls={visible ? listId : undefined}
      aria-activedescendant={visible && highlight >= 0 ? `${listId}-${highlight}` : undefined}
      aria-autocomplete="list"
      aria-invalid={error ? true : undefined}
      aria-describedby={error ? errorId : describedBy}
      class="h-6 min-w-24 flex-1 bg-transparent px-1 text-base outline-none placeholder:text-muted-foreground md:text-sm"
      oninput={handleInput}
      onkeydown={handleKeydown}
      onfocus={() => (open = true)}
      onblur={handleBlur}
    />
  </div>
  <Popover.Content
    class="w-(--bits-popover-anchor-width) p-1"
    align="start"
    customAnchor={boxRef}
    trapFocus={false}
    onOpenAutoFocus={keepFocus}
    onCloseAutoFocus={keepFocus}
    onInteractOutside={handleInteractOutside}
  >
    <ul id={listId} class="flex max-h-64 flex-col overflow-y-auto" role="listbox">
      {#each suggestions as option, i (option)}
        <li>
          <button
            id="{listId}-{i}"
            type="button"
            role="option"
            aria-selected={highlight === i}
            tabindex="-1"
            class={cn(
              'flex w-full cursor-default items-center rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none',
              highlight === i && 'bg-accent text-accent-foreground'
            )}
            onmousedown={keepFocus}
            onclick={() => {
              commit(option);
              inputRef?.focus();
            }}
            onpointermove={(e) => hoverOption(e, i)}
          >
            <span class="truncate">{option}</span>
          </button>
        </li>
      {/each}
    </ul>
  </Popover.Content>
</Popover.Root>
{#if error}
  <p id={errorId} role="alert" class="text-xs text-destructive">{error}</p>
{/if}
