<script lang="ts">
  import {
    hierarchy,
    type HierarchyNode,
    type HierarchyPointLink,
    type HierarchyPointNode
  } from 'd3-hierarchy';
  import { resolve } from '$app/paths';
  import { SvelteSet } from 'svelte/reactivity';
  import { Chart, Group, Layer, Link, Tooltip as ChartTooltip } from 'layerchart';
  import { Tree } from 'layerchart/hierarchy';
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';
  import { clampZero, formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import type { AccountNode } from '$lib/tauri';

  let { data, loading }: { data: AccountNode[]; loading: boolean } = $props();

  interface TreeDatum {
    guid?: string;
    name: string;
    description?: string | null;
    accountType?: string;
    balance?: number;
    txCount?: number;
    placeholder?: boolean;
    children?: TreeDatum[];
  }

  // Full tree with aggregate (subtree) balances, fully expanded. Placeholder
  // parents contribute 0: they hold no splits themselves and their `balance`
  // already carries propagated child balances, so summing it would double
  // count. `.sum()` adds children recursively, so each node's value is its
  // subtree total with every account's own balance counted exactly once.
  let root = $derived(
    hierarchy<TreeDatum>({ name: 'root', children: data }, (d) => d.children).sum((d) =>
      d.placeholder ? 0 : Math.abs(clampZero(d.balance ?? 0))
    )
  );

  // Distinct account types present in the tree, in traversal order, for the legend.
  let legendTypes = $derived.by(() => {
    const seen = new SvelteSet<string>();
    const types: string[] = [];
    const walk = (nodes: AccountNode[]): void => {
      for (const node of nodes) {
        if (!seen.has(node.accountType)) {
          seen.add(node.accountType);
          types.push(node.accountType);
        }
        if (node.children.length > 0) walk(node.children);
      }
    };
    walk(data);
    return types;
  });

  function nodeColor(node: HierarchyPointNode<TreeDatum>): string {
    if (!node.data.guid) return 'var(--color-primary)';
    return TYPE_COLOR[node.data.accountType ?? ''] ?? 'var(--color-muted-foreground)';
  }

  function nodeFill(node: HierarchyPointNode<TreeDatum>): string {
    return `color-mix(in srgb, ${nodeColor(node)} 50%, var(--color-background))`;
  }

  function nodeLabel(node: HierarchyPointNode<TreeDatum>): string {
    return node.data.description || node.data.name;
  }

  function nodeBalance(node: HierarchyNode<TreeDatum>): number {
    return node.data.children?.length ? (node.value ?? 0) : (node.data.balance ?? 0);
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
      <Skeleton class="mx-auto h-160 w-full max-w-160" />
    {:else if data.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">No account balances found.</div>
    {:else}
      <div class="flex flex-col gap-4">
        <div class="mx-auto h-120 w-full max-w-160">
          <Chart radial>
            {#snippet children({ context })}
              <Tree hierarchy={root} orientation="horizontal">
                {#snippet children({ nodes, links })}
                  <Layer>
                    <Group center>
                      {#each links as link (linkKey(link))}
                        <Link data={link} type="straight" opacity={0.2} />
                      {/each}
                    </Group>
                  </Layer>

                  <Layer type="html">
                    <Group center>
                      {#each nodes as node (node.data.guid ?? 'root')}
                        {@const r = 10}
                        {@const color = nodeColor(node)}
                        {@const label = nodeLabel(node)}
                        {@const x = node.y * Math.sin(node.x) - r}
                        {@const y = -node.y * Math.cos(node.x) - r}
                        <Group
                          {x}
                          {y}
                          motion="tween"
                          style={`width: ${r * 2}px; height: ${r * 2}px;`}
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
                              class="block h-full w-full cursor-pointer rounded-full border hover:scale-120"
                              style={`background-color: ${nodeFill(node)}; border-color: ${color};`}
                              aria-label={label}
                            ></a>
                          {:else}
                            <div
                              class="h-full w-full rounded-full border hover:scale-120"
                              style={`background-color: ${nodeFill(node)}; border-color: ${color};`}
                            ></div>
                          {/if}
                        </Group>
                      {/each}
                    </Group>
                  </Layer>
                {/snippet}
              </Tree>
              <ChartTooltip.Root variant="none">
                {#snippet children({ data: node })}
                  <div
                    class="flex gap-2 rounded-lg bg-popover p-2.5 text-xs text-popover-foreground shadow-md ring-1 ring-foreground/10"
                  >
                    <div class="font-medium text-muted-foreground">{nodeLabel(node)}</div>
                    <span class="font-medium tabular-nums">{fmt(clampZero(nodeBalance(node)))}</span
                    >
                  </div>
                {/snippet}
              </ChartTooltip.Root>
            {/snippet}
          </Chart>
        </div>
        {#if legendTypes.length > 0}
          <aside
            aria-label="Account type legend"
            class="flex shrink-0 flex-wrap justify-center gap-x-4 gap-y-2"
          >
            {#each legendTypes as type (type)}
              <div class="flex items-center gap-2">
                <span
                  class="size-2.5 shrink-0 rounded-full"
                  style={`background-color: ${TYPE_COLOR[type] ?? 'var(--color-muted-foreground)'}`}
                ></span>
                <span class="text-xs text-muted-foreground">{type}</span>
              </div>
            {/each}
          </aside>
        {/if}
      </div>
    {/if}
  </Card.Content>
</Card.Root>
