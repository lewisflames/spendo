<script lang="ts">
  import { ChevronRight } from '@lucide/svelte';

  let {
    open = false,
    interactive = true,
    ontoggle
  }: {
    open?: boolean;
    interactive?: boolean;
    ontoggle?: () => void;
  } = $props();
</script>

{#if interactive}
  <button
    type="button"
    class="flex size-4 shrink-0 items-center justify-center rounded hover:bg-muted-foreground/10"
    aria-expanded={open}
    aria-label={open ? 'Collapse' : 'Expand'}
    onclick={(e) => {
      e.stopPropagation();
      ontoggle?.();
    }}
  >
    <ChevronRight
      aria-hidden="true"
      class="size-3 text-muted-foreground transition-transform {open ? 'rotate-90' : ''}"
    />
  </button>
{:else}
  <div class="size-4 shrink-0"></div>
{/if}
