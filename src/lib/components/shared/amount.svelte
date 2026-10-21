<script lang="ts">
  import { cn } from '$lib/utils';
  import { clampZero, formatSessionCurrency, signColor } from '$lib/utils/format.svelte';

  /**
   * One money value, rendered the same way everywhere: tabular digits, no
   * wrapping, -0 clamped to 0 and an optional gain/loss tint. Pass `format`
   * for signed or foreign-currency output; the default is the session currency.
   */
  let {
    value,
    format = formatSessionCurrency,
    tone = false,
    positiveGood = true,
    weight = 'medium',
    class: className = ''
  }: {
    value: number;
    format?: (n: number) => string;
    /** Tint positive/negative values (success/destructive). */
    tone?: boolean;
    positiveGood?: boolean;
    weight?: 'normal' | 'medium' | 'semibold';
    class?: string;
  } = $props();

  let n = $derived(clampZero(value));
</script>

<span
  class={cn(
    'whitespace-nowrap tabular-nums',
    weight === 'medium' && 'font-medium',
    weight === 'semibold' && 'font-semibold',
    tone && signColor(n, positiveGood),
    className
  )}>{format(n)}</span
>
