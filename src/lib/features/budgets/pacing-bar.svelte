<script lang="ts">
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { Progress } from '$lib/components/ui/progress';
  import { cn } from '$lib/utils';

  let {
    pacingPercentage,
    actualAmount,
    expectedYtd,
    delta,
    fmt,
    size = 'sm',
    tooltip = true
  }: {
    pacingPercentage: number;
    actualAmount: number;
    expectedYtd: number;
    delta: number;
    fmt: (n: number) => string;
    size?: 'sm' | 'md';
    tooltip?: boolean;
  } = $props();

  let barClass = $derived(size === 'md' ? 'h-2 flex-1' : 'h-1.5 w-20');
  let pctClass = $derived(size === 'md' ? 'text-sm tabular-nums' : 'text-xs tabular-nums');
</script>

{#if tooltip}
  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props })}
        <span {...props} class="flex items-center gap-2">
          <Progress
            value={Math.min(100, Math.max(0, pacingPercentage))}
            max={100}
            class={cn(
              barClass,
              pacingPercentage > 100 &&
                'bg-destructive/20 **:data-[slot=progress-indicator]:bg-destructive'
            )}
          />
          <span
            class={cn(
              pctClass,
              pacingPercentage > 100 ? 'font-medium text-destructive' : 'text-muted-foreground'
            )}>{Math.max(0, pacingPercentage).toFixed(1)}%</span
          >
        </span>
      {/snippet}
    </Tooltip.Trigger>
    <Tooltip.Content>
      <p>
        {fmt(actualAmount)} spent of {fmt(expectedYtd)} expected
        {#if delta >= 0}
          ({fmt(delta)} under)
        {:else}
          ({fmt(-delta)} over)
        {/if}
      </p>
    </Tooltip.Content>
  </Tooltip.Root>
{:else}
  <span class="flex items-center gap-2">
    <Progress
      value={Math.min(100, Math.max(0, pacingPercentage))}
      max={100}
      class={cn(
        barClass,
        pacingPercentage > 100 &&
          'bg-destructive/20 **:data-[slot=progress-indicator]:bg-destructive'
      )}
    />
    <span
      class={cn(
        pctClass,
        pacingPercentage > 100 ? 'font-medium text-destructive' : 'text-muted-foreground'
      )}>{Math.max(0, pacingPercentage).toFixed(1)}%</span
    >
  </span>
{/if}
