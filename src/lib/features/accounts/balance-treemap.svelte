<script lang="ts">
  import { hierarchy, type HierarchyNode, type HierarchyRectangularNode } from 'd3-hierarchy';
  import { Chart, Group, Layer, Rect, RectClipPath, Text, Tooltip } from 'layerchart';
  import { Treemap } from 'layerchart/hierarchy';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Card from '$lib/components/ui/card';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { nodeCategory } from '$lib/features/accounts/account-types';
  import { clampZero, formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import type { AccountNode } from '$lib/tauri';

  let { data, loading }: { data: AccountNode[]; loading: boolean } = $props();

  interface TreemapDatum {
    name: string;
    guid?: string;
    category: Category;
    color: string;
    balance?: number;
    children?: TreemapDatum[];
  }

  type Category = 'ASSET' | 'LIABILITY';

  const GROUPS: { key: Category; label: string; color: string }[] = [
    { key: 'ASSET', label: 'Assets', color: 'var(--color-success)' },
    { key: 'LIABILITY', label: 'Liabilities', color: 'var(--color-destructive)' }
  ];

  function prune(node: AccountNode, color: string, category: Category): TreemapDatum | null {
    const children = node.children
      .map((child) => prune(child, color, category))
      .filter((c): c is TreemapDatum => c !== null);
    const name = node.description || node.name;
    if (children.length > 0) {
      return { name, category, color, children };
    }
    // Drop zero-balance leaves; they would render as invisible slivers.
    if (Math.abs(clampZero(node.balance)) < 0.005) return null;
    return { name, guid: node.guid, category, color, balance: node.balance };
  }

  let root = $derived.by(() => {
    const groups: TreemapDatum[] = [];
    for (const group of GROUPS) {
      const children: TreemapDatum[] = [];
      for (const tree of data.filter((t) => nodeCategory(t.accountType) === group.key)) {
        const pruned = prune(tree, group.color, group.key);
        if (!pruned) continue;
        // Top-level accounts duplicate their category group ("Assets" inside
        // Assets), so splice their children directly into the group.
        if (pruned.children) children.push(...pruned.children);
        else children.push(pruned);
      }
      if (children.length > 0) {
        groups.push({ name: group.label, category: group.key, color: group.color, children });
      }
    }
    if (groups.length === 0) return null;
    // Synthetic root; never rendered (tiles filter to depth > 0).
    const root = hierarchy<TreemapDatum>({
      name: 'Balances',
      category: 'ASSET',
      color: 'transparent',
      children: groups
    });
    root.sum((d) => (d.children ? 0 : Math.abs(d.balance ?? 0)));
    root.sort(
      (a: HierarchyNode<TreemapDatum>, b: HierarchyNode<TreemapDatum>) =>
        (b.value ?? 0) - (a.value ?? 0)
    );
    return root;
  });

  function nodeKey(node: HierarchyRectangularNode<TreemapDatum>): string {
    const parts = [node.data.guid ?? node.data.name];
    let p = node.parent;
    while (p) {
      parts.unshift(p.data.name);
      p = p.parent;
    }
    return parts.join('/');
  }

  function groupLabel(category: Category): string {
    return GROUPS.find((g) => g.key === category)?.label ?? category;
  }

  function shareOfGroup(node: HierarchyRectangularNode<TreemapDatum>): number | null {
    if (node.depth < 2 || !node.parent || !node.parent.value) return null;
    return ((node.value ?? 0) / node.parent.value) * 100;
  }
</script>

<Card.Root>
  <Card.Header>
    <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <div class="flex min-w-0 flex-col gap-1">
        <Card.Title>Balance Composition</Card.Title>
        <Card.Description>Assets and liabilities by account size</Card.Description>
      </div>
      <div class="flex items-center gap-4">
        {#each GROUPS as g (g.key)}
          <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span
              class="size-2.5 shrink-0 rounded-[2px] border"
              style="background-color: color-mix(in srgb, {g.color} 22%, transparent); border-color: {g.color};"
            ></span>
            {g.label}
          </span>
        {/each}
      </div>
    </div>
  </Card.Header>
  <Card.Content>
    {#if loading}
      <Skeleton class="h-72 w-full" />
    {:else if !root}
      <div class="py-6 text-center text-sm text-muted-foreground">No account balances found.</div>
    {:else}
      <div class="h-72 w-full">
        <Chart>
          {#snippet children({ context })}
            <Layer>
              <Treemap
                hierarchy={root}
                tile="squarify"
                paddingInner={2}
                paddingOuter={2}
                paddingTop={16}
              >
                {#snippet children({ nodes })}
                  {#each nodes.filter((n) => n.depth > 0) as node (nodeKey(node))}
                    {@const w = node.x1 - node.x0}
                    {@const h = node.y1 - node.y0}
                    {@const isLeaf = !node.children}
                    {@const fill = `color-mix(in srgb, ${node.data.color} ${isLeaf ? 22 : 12}%, transparent)`}
                    <Group
                      x={node.x0}
                      y={node.y0}
                      class={isLeaf ? 'cursor-pointer' : ''}
                      onpointermove={(e) => context.tooltip.show(e, node)}
                      onpointerleave={() => context.tooltip.hide()}
                      onclick={() => {
                        if (node.data.guid) {
                          goto(resolve('/accounts/[guid]', { guid: node.data.guid }));
                        }
                      }}
                    >
                      <Rect
                        width={w}
                        height={h}
                        rx={4}
                        {fill}
                        stroke={node.data.color}
                        strokeOpacity={0.4}
                      />
                      {#if isLeaf && w > 56 && h > 28}
                        <RectClipPath width={w} height={h} rx={4}>
                          <Text
                            x={w / 2}
                            y={h > 52 ? h / 2 - 7 : h / 2}
                            value={node.data.name}
                            fill={node.data.color}
                            textAnchor="middle"
                            verticalAnchor="middle"
                            fontSize={11}
                            class="font-medium"
                          />
                          {#if h > 52}
                            <Text
                              x={w / 2}
                              y={h / 2 + 9}
                              value={fmt(clampZero(node.data.balance ?? 0))}
                              textAnchor="middle"
                              verticalAnchor="middle"
                              fontSize={10}
                              class="tabular-nums"
                            />
                          {/if}
                        </RectClipPath>
                      {:else if !isLeaf && w > 64}
                        <Text
                          x={4}
                          y={3}
                          value={node.data.name}
                          fill={node.data.color}
                          verticalAnchor="start"
                          fontSize={10}
                          class="font-medium"
                        />
                      {/if}
                    </Group>
                  {/each}
                {/snippet}
              </Treemap>
            </Layer>
            <Tooltip.Root variant="none">
              {#snippet children({ data: node })}
                {@const share = shareOfGroup(node)}
                <div
                  class="grid min-w-[10rem] gap-2 rounded-lg bg-popover p-2.5 text-xs text-popover-foreground shadow-md ring-1 ring-foreground/10"
                >
                  <div class="flex items-center gap-1.5 font-medium">
                    <span
                      class="size-2.5 shrink-0 rounded-[2px]"
                      style="background-color: {node.data.color};"
                    ></span>
                    <span class="truncate">{node.data.name}</span>
                  </div>
                  <ChartTooltipRow
                    label={groupLabel(node.data.category)}
                    value={fmt(clampZero(node.data.balance ?? node.value ?? 0))}
                    valueSuffix={share !== null ? `${share.toFixed(1)}%` : undefined}
                  />
                </div>
              {/snippet}
            </Tooltip.Root>
          {/snippet}
        </Chart>
      </div>
    {/if}
  </Card.Content>
</Card.Root>
