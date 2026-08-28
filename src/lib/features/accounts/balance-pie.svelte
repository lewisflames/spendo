<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Chart from '$lib/components/ui/chart';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { PieChart } from 'layerchart';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';
  import { clampZero } from '$lib/utils/format.svelte';
  import type { AccountNode, ParentAccountInfo } from '$lib/tauri';

  let {
    node,
    parent,
    loading
  }: {
    node: AccountNode | null;
    parent: ParentAccountInfo | null;
    loading: boolean;
  } = $props();

  interface PieDatum {
    key: string;
    label: string;
    value: number;
    color: string;
  }

  let pieData = $derived.by((): PieDatum[] => {
    if (!node) return [];
    return node.children
      .filter((c) => Math.abs(clampZero(c.balance)) >= 0.005)
      .map((c) => ({
        key: c.guid,
        label: c.description ?? c.name,
        value: Math.abs(clampZero(c.balance)),
        color: TYPE_COLOR[c.accountType] ?? 'var(--color-muted-foreground)'
      }));
  });

  let chartConfig: Chart.ChartConfig = $derived.by(() => {
    const cfg: Chart.ChartConfig = {};
    for (const d of pieData) {
      cfg[d.key] = { label: d.label };
    }
    return cfg;
  });

  let accountBalance = $derived(Math.abs(clampZero(node?.balance ?? 0)));

  let parentSharePct = $derived.by((): number | null => {
    if (!parent || Math.abs(clampZero(parent.balance)) < 0.005) return null;
    return (accountBalance / Math.abs(clampZero(parent.balance))) * 100;
  });

  let parentLabel = $derived(parent?.description ?? parent?.name ?? '');
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Balance Composition</Card.Title>
    <Card.Description>Sub-account balances by type.</Card.Description>
  </Card.Header>
  <Card.Content>
    {#if loading}
      <Skeleton class="h-72 w-full" />
    {:else if pieData.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">
        No sub-account balances found.
      </div>
    {:else}
      <div class="relative">
        <Chart.Container config={chartConfig} class="aspect-auto h-72 w-full">
          <PieChart
            data={pieData}
            key="key"
            label="label"
            value="value"
            innerRadius={0.6}
            cornerRadius={4}
            padAngle={0.035}
            c="color"
          >
            {#snippet tooltip()}
              <Chart.Tooltip hideLabel>
                {#snippet formatter({ item })}
                  {@const child = node?.children.find((c) => c.guid === item.key)}
                  <ChartTooltipRow
                    color={item.color as string}
                    label={child
                      ? (child.description ?? child.name)
                      : String(item.label ?? item.key)}
                    value={child && accountBalance > 0
                      ? `${((Math.abs(clampZero(child.balance)) / accountBalance) * 100).toFixed(1)}%`
                      : undefined}
                  />
                {/snippet}
              </Chart.Tooltip>
            {/snippet}
          </PieChart>
        </Chart.Container>
        {#if parentSharePct !== null}
          <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div class="text-center">
              <div class="text-xs text-muted-foreground">of {parentLabel}</div>
              <div class="text-sm font-medium tabular-nums">{parentSharePct.toFixed(1)}%</div>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </Card.Content>
</Card.Root>
