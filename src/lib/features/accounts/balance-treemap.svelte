<script lang="ts">
  import {
    hierarchy,
    treemap as d3treemap,
    treemapSquarify,
    type HierarchyNode,
    type HierarchyRectangularNode
  } from 'd3-hierarchy';
  import { cubicOut } from 'svelte/easing';
  import { fade } from 'svelte/transition';
  import { ZoomIn, ZoomOut, RotateCcw } from '@lucide/svelte';
  import {
    asAny,
    aspectTile,
    Bounds,
    Chart,
    ChartClipPath,
    Group,
    isNodeVisible,
    Layer,
    Rect,
    RectClipPath,
    Text,
    Tooltip as ChartTooltip
  } from 'layerchart';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';
  import { clampZero, formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import type { AccountNode } from '$lib/tauri';

  let { subtree, loading }: { subtree: AccountNode | null; loading: boolean } = $props();

  interface TreemapDatum {
    name: string;
    guid?: string;
    color: string;
    balance?: number;
    children?: TreemapDatum[];
  }

  function colorFor(accountType: string): string {
    return TYPE_COLOR[accountType] ?? 'var(--color-muted-foreground)';
  }

  function prune(node: AccountNode): TreemapDatum | null {
    const children = node.children.map(prune).filter((c): c is TreemapDatum => c !== null);
    const name = node.description || node.name;
    const color = colorFor(node.accountType);
    if (children.length > 0) {
      return { name, guid: node.guid, color, children };
    }
    // Drop zero-balance leaves; they would render as invisible slivers.
    if (Math.abs(clampZero(node.balance)) < 0.005) return null;
    return { name, guid: node.guid, color, balance: node.balance };
  }

  let root = $derived.by(() => {
    if (!subtree) return null;
    const pruned = prune(subtree);
    if (!pruned) return null;
    const h = hierarchy<TreemapDatum>(pruned);
    h.sum((d) => (d.children ? 0 : Math.abs(d.balance ?? 0)));
    h.sort(
      (a: HierarchyNode<TreemapDatum>, b: HierarchyNode<TreemapDatum>) =>
        (b.value ?? 0) - (a.value ?? 0)
    );
    return h;
  });

  // Chart area dimensions; drive the layout. Treemap component would lay out
  // its own copy, so instead the layout runs here to keep the node references
  // that `selected` points at stable across zooms and resizes.
  let width = $state(0);
  let height = $state(0);

  let layout = $derived.by((): HierarchyRectangularNode<TreemapDatum>[] => {
    if (!root || width === 0 || height === 0) return [];
    const treemapLayout = d3treemap<TreemapDatum>()
      .size([width, height])
      .tile(aspectTile(treemapSquarify, width, height));
    const laid = treemapLayout(root);
    return laid.descendants();
  });

  // Path of node keys from the root; null means the root itself. Keyed by path
  // rather than node reference so a data refresh (new hierarchy) falls back to
  // the root when the path no longer resolves.
  let selectedPath = $state<string[] | null>(null);

  let selected = $derived.by((): HierarchyNode<TreemapDatum> | null => {
    if (!root) return null;
    if (!selectedPath) return root;
    const key = selectedPath.join('/');
    return root.descendants().find((n) => nodeKey(n) === key) ?? root;
  });

  function nodeKey(node: HierarchyNode<TreemapDatum>): string {
    const parts = [node.data.guid ?? node.data.name];
    let p = node.parent;
    while (p) {
      parts.unshift(p.data.name);
      p = p.parent;
    }
    return parts.join('/');
  }

  // Hovered tile; drives the zoom-badge affordance and lets the Zoom-in
  // button target a specific group. Cleared on zoom to avoid stale badges
  // during the transition.
  let hovered = $state<HierarchyRectangularNode<TreemapDatum> | null>(null);

  function zoomTo(node: HierarchyNode<TreemapDatum>) {
    selectedPath = nodeKey(node).split('/');
    hovered = null;
  }

  function handleTileClick(node: HierarchyRectangularNode<TreemapDatum>) {
    if (node.children) zoomTo(node);
  }

  // Zoom-in target: the hovered group, else the largest child of the
  // selection so the button is always usable when children exist.
  let zoomInTarget = $derived.by((): HierarchyNode<TreemapDatum> | null => {
    if (hovered?.children) return hovered;
    const children = selected?.children ?? [];
    let largest: HierarchyNode<TreemapDatum> | null = null;
    for (const child of children) {
      if (!largest || (child.value ?? 0) > (largest.value ?? 0)) largest = child;
    }
    return largest;
  });

  let canZoomOut = $derived(!!selected?.parent);

  function zoomIn() {
    if (zoomInTarget) zoomTo(zoomInTarget);
  }

  function zoomOut() {
    const parent = selected?.parent;
    selectedPath = parent && parent.parent ? nodeKey(parent).split('/') : null;
    hovered = null;
  }

  function resetZoom() {
    selectedPath = null;
    hovered = null;
  }

  // Focused tiles (children of the selected node) are stronger; the siblings
  // of the selected node's ancestors shrink into the background and are dimmed.
  function tileFill(node: HierarchyRectangularNode<TreemapDatum>): string {
    const focused = node.parent === selected;
    return `color-mix(in srgb, ${node.data.color} ${focused ? 40 : 15}%, var(--color-background))`;
  }

  function shareOfGroup(node: HierarchyRectangularNode<TreemapDatum>): number | null {
    if (node.depth < 1 || !node.parent || !node.parent.value) return null;
    return ((node.value ?? 0) / node.parent.value) * 100;
  }
</script>

<Card.Root>
  <Card.Header>
    <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <div class="flex min-w-0 flex-col gap-1">
        <Card.Title>Balance Composition</Card.Title>
        <Card.Description>
          Sub-account balances colored by account type. Click a group or use the controls to zoom.
        </Card.Description>
      </div>
      <ButtonGroup.Root class="shrink-0">
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="outline"
                size="icon"
                onclick={zoomIn}
                disabled={!zoomInTarget}
                aria-label="Zoom in"
              >
                <ZoomIn aria-hidden="true" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>Zoom in</Tooltip.Content>
        </Tooltip.Root>
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="outline"
                size="icon"
                onclick={zoomOut}
                disabled={!canZoomOut}
                aria-label="Zoom out"
              >
                <ZoomOut aria-hidden="true" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>Zoom out</Tooltip.Content>
        </Tooltip.Root>
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="outline"
                size="icon"
                onclick={resetZoom}
                disabled={!selectedPath}
                aria-label="Reset zoom"
              >
                <RotateCcw aria-hidden="true" />
              </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content>Reset zoom</Tooltip.Content>
        </Tooltip.Root>
      </ButtonGroup.Root>
    </div>
  </Card.Header>
  <Card.Content>
    {#if loading}
      <Skeleton class="h-72 w-full" />
    {:else if !root}
      <div class="py-6 text-center text-sm text-muted-foreground">No account balances found.</div>
    {:else}
      <div class="h-96 w-full" bind:clientWidth={width} bind:clientHeight={height}>
        <Chart>
          {#snippet children({ context })}
            <Layer>
              <Bounds
                domain={asAny(selected)}
                motion={{ type: 'tween', duration: 800, easing: cubicOut }}
              >
                {#snippet children({ xScale, yScale })}
                  <ChartClipPath>
                    {#each layout.filter((n) => isNodeVisible(n, selected)) as node (nodeKey(node))}
                      {@const w = xScale(node.x1) - xScale(node.x0)}
                      {@const h = yScale(node.y1) - yScale(node.y0)}
                      {@const isLeaf = !node.children}
                      {@const fill = tileFill(node)}
                      <Group
                        x={xScale(node.x0)}
                        y={yScale(node.y0)}
                        class={isLeaf ? '' : 'cursor-pointer'}
                        onpointermove={(e) => {
                          hovered = node;
                          context.tooltip.show(e, node);
                        }}
                        onpointerleave={() => {
                          hovered = null;
                          context.tooltip.hide();
                        }}
                        onclick={() => handleTileClick(node)}
                      >
                        <g transition:fade={{ duration: 250 }}>
                          <Rect
                            width={w}
                            height={h}
                            rx={4}
                            {fill}
                            stroke={node.data.color}
                            strokeOpacity={0.4}
                          />
                          {#if !isLeaf && h > 28 && w > 28}
                            <g transform="translate({w - 22}, 6)">
                              <rect
                                width="16"
                                height="16"
                                rx="4"
                                fill="var(--color-background)"
                                stroke={node.data.color}
                                stroke-opacity="0.5"
                              />
                              <ZoomIn x={2} y={2} size={12} color={node.data.color} />
                            </g>
                          {/if}
                          {#if w > 56 && h > 28}
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
                                  value={fmt(
                                    clampZero(isLeaf ? (node.data.balance ?? 0) : (node.value ?? 0))
                                  )}
                                  fill={node.data.color}
                                  textAnchor="middle"
                                  verticalAnchor="middle"
                                  fontSize={10}
                                  class="tabular-nums"
                                />
                              {/if}
                            </RectClipPath>
                          {/if}
                        </g>
                      </Group>
                    {/each}
                  </ChartClipPath>
                {/snippet}
              </Bounds>
            </Layer>
            <ChartTooltip.Root variant="none">
              {#snippet children({ data: node })}
                {@const share = shareOfGroup(node)}
                <div
                  class="grid gap-2 rounded-lg bg-popover p-2.5 text-xs text-popover-foreground shadow-md ring-1 ring-foreground/10"
                >
                  <div class="flex items-center gap-1.5 font-medium">
                    <span
                      class="size-2.5 shrink-0 rounded-[2px]"
                      style="background-color: {node.data.color};"
                    ></span>
                    <span class="truncate">{node.data.name}</span>
                  </div>
                  <ChartTooltipRow
                    label="Balance"
                    value={fmt(clampZero(node.data.balance ?? node.value ?? 0))}
                  />
                  {#if share !== null}
                    <ChartTooltipRow label="Ratio" value={`${share.toFixed(1)}%`} />
                  {/if}
                  {#if node.children?.length}
                    <ChartTooltipRow label="Subaccounts" value={`x${node.children.length}`} />
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
