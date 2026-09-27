<script lang="ts">
  import type { Snippet } from 'svelte';
  import * as Table from '$lib/components/ui/table';
  import { handleRowClick } from '$lib/utils/navigation';

  /**
   * Clickable table row with row-click navigation. Renders a real
   * `Table.Row`; keep a real `<a href>` inside a cell for
   * keyboard/middle-click — the row click is only a pointer shortcut and the
   * guard ignores clicks on interactive children. Pass `onNavigate: null` (or
   * omit) for non-navigating rows such as synthetic totals. `onclickcapture`
   * runs in the capture phase (e.g. shift-click range selection).
   */
  let {
    onNavigate,
    onclickcapture,
    class: className = 'cursor-pointer hover:bg-muted/50',
    children
  }: {
    onNavigate?: ((event: MouseEvent) => void) | null;
    onclickcapture?: ((event: MouseEvent) => void) | null;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<Table.Row
  class={className}
  onclick={onNavigate ? (e) => handleRowClick(e, () => onNavigate(e)) : undefined}
  onclickcapture={onclickcapture ?? undefined}
>
  {@render children()}
</Table.Row>
