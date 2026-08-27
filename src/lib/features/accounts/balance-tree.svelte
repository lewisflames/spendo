<script lang="ts">
  import { hierarchy, type HierarchyPointLink, type HierarchyPointNode } from 'd3-hierarchy';
  import { resolve } from '$app/paths';
  import { Chart, Group, Layer, Link, Tooltip as ChartTooltip } from 'layerchart';
  import { Tree } from 'layerchart/hierarchy';
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import {
    CATEGORY_LABEL_SINGULAR,
    TYPE_COLOR,
    nodeCategory
  } from '$lib/features/accounts/account-types';
  import { clampZero, formatNumber, formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import type { AccountNode } from '$lib/tauri';

  let { data, loading }: { data: AccountNode[]; loading: boolean } = $props();

  interface TreeDatum {
    guid?: string;
    name: string;
    description?: string | null;
    accountType?: string;
    balance?: number;
    txCount?: number;
    children?: TreeDatum[];
  }

  const NODE_WIDTH = 60;
  const MIN_NODE_WIDTH = 120;
  const NODE_GAP = 20;
  const NODE_HEIGHT = 20;

  // Full tree with aggregate (subtree) balances, fully expanded.
  let root = $derived(
    hierarchy<TreeDatum>({ name: 'root', children: data }, (d) => d.children).sum((d) =>
      d.children ? 0 : Math.abs(clampZero(d.balance ?? 0))
    )
  );

  function nodeColor(node: HierarchyPointNode<TreeDatum>): string {
    if (!node.data.guid) return 'var(--color-primary)';
    return TYPE_COLOR[node.data.accountType ?? ''] ?? 'var(--color-muted-foreground)';
  }

  function nodeFill(node: HierarchyPointNode<TreeDatum>): string {
    return `color-mix(in srgb, ${nodeColor(node)} 40%, var(--color-background))`;
  }

  function nodeLabel(node: HierarchyPointNode<TreeDatum>): string {
    return node.data.description || node.data.name;
  }

  function nodeBalance(node: HierarchyPointNode<TreeDatum>): number {
    return node.data.children?.length ? (node.value ?? 0) : (node.data.balance ?? 0);
  }

  function linkColor(link: HierarchyPointLink<TreeDatum>): string {
    return nodeColor(link.target);
  }

  // Smallest horizontal gap between sibling nodes at each depth; node widths
  // shrink so dense layouts never overlap.
  function levelGaps(nodes: HierarchyPointNode<TreeDatum>[]): Record<number, number> {
    const positions: Record<number, number[]> = {};
    for (const node of nodes) {
      (positions[node.depth] ??= []).push(node.y);
    }
    const gaps: Record<number, number> = {};
    for (const [depth, ys] of Object.entries(positions)) {
      ys.sort((a, b) => a - b);
      let gap = Infinity;
      for (let i = 1; i < ys.length; i++) gap = Math.min(gap, ys[i] - ys[i - 1]);
      gaps[Number(depth)] = gap;
    }
    return gaps;
  }

  function nodeWidth(node: HierarchyPointNode<TreeDatum>, gaps: Record<number, number>): number {
    const gap = gaps[node.depth] ?? Infinity;
    return Math.max(MIN_NODE_WIDTH, Math.min(NODE_WIDTH, gap - NODE_GAP));
  }

  function linkKey(link: HierarchyPointLink<TreeDatum>): string {
    return `${link.source.data.guid ?? 'root'}>${link.target.data.guid}`;
  }
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Account Hierarchy</Card.Title>
    <Card.Description>Account balances by type. Click a node to open its account.</Card.Description>
  </Card.Header>
  <Card.Content>
    {#if loading}
      <Skeleton class="h-120 w-full" />
    {:else if data.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">No account balances found.</div>
    {:else}
      <div class="h-120 w-full">
        <Chart padding={{ right: 72, left: 72 }}>
          {#snippet children({ context })}
            <Tree hierarchy={root} orientation="horizontal">
              {#snippet children({ nodes, links })}
                {@const gaps = levelGaps(nodes)}
                <Layer>
                  {#each links as link (linkKey(link))}
                    <Link
                      data={link}
                      orientation="horizontal"
                      motion="tween"
                      stroke={linkColor(link)}
                      class="opacity-20"
                    />
                  {/each}
                </Layer>

                <Layer type="html">
                  {#each nodes as node (node.data.guid ?? 'root')}
                    {@const w = nodeWidth(node, gaps)}
                    {@const color = nodeColor(node)}
                    {@const label = nodeLabel(node)}
                    <Group
                      x={node.y - w / 2}
                      y={node.x - NODE_HEIGHT / 2}
                      motion="tween"
                      style={`width: ${w}px; height: ${NODE_HEIGHT}px;`}
                      onpointermove={(e) => {
                        context.tooltip.show(e, node);
                      }}
                      onpointerleave={() => {
                        context.tooltip.hide();
                      }}
                    >
                      {#if node.data.guid}
                        <a
                          href={resolve('/accounts/[guid]', { guid: node.data.guid })}
                          class="flex h-full w-full cursor-pointer items-center justify-center rounded-full border text-[11px] font-medium focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                          style={`background-color: ${nodeFill(node)}; border-color: ${color}; color: ${color};`}
                        >
                          <span class="min-w-0 truncate px-1.5">{label}</span>
                        </a>
                      {:else}
                        <div
                          class="flex h-full w-full items-center justify-center rounded-full border text-[11px] font-medium"
                          style={`background-color: ${nodeFill(node)}; border-color: ${color}; color: ${color};`}
                        >
                          <span class="min-w-0 truncate px-1.5">{label}</span>
                        </div>
                      {/if}
                    </Group>
                  {/each}
                </Layer>
              {/snippet}
            </Tree>
            <ChartTooltip.Root variant="none">
              {#snippet children({ data: node })}
                <div
                  class="grid gap-2 rounded-lg bg-popover p-2.5 text-xs text-popover-foreground shadow-md ring-1 ring-foreground/10"
                >
                  <div class="flex items-center gap-1.5 font-medium">
                    <span
                      class="size-2.5 shrink-0 rounded-[2px]"
                      style="background-color: {nodeColor(node)};"
                    ></span>
                    <span class="truncate">{nodeLabel(node)}</span>
                  </div>
                  <ChartTooltipRow label="Balance" value={fmt(clampZero(nodeBalance(node)))} />
                  {#if node.data.guid}
                    <ChartTooltipRow
                      label="Category"
                      value={CATEGORY_LABEL_SINGULAR[nodeCategory(node.data.accountType ?? '')] ??
                        node.data.accountType}
                    />
                  {/if}
                  {#if node.data.txCount}
                    <ChartTooltipRow label="Transactions" value={formatNumber(node.data.txCount)} />
                  {/if}
                </div>
              {/snippet}
            </ChartTooltip.Root>
          {/snippet}
        </Chart>
      </div>
    {/if}
  </Card.Content>
</Card.Root>
