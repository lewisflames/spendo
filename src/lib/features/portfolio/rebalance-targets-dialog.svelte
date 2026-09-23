<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { toast } from '$lib/components/ui/sonner';
  import SaveFooter from '$lib/components/shared/save-footer.svelte';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { saveAllocationTargets, type PortfolioHolding } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { formatAllocPct, formatCurrency, plural } from '$lib/utils/format.svelte';
  import { computeRebalance } from './rebalance';

  let {
    open = $bindable(false),
    holdings,
    currency,
    targets,
    onSaved
  }: {
    open: boolean;
    holdings: PortfolioHolding[];
    currency: string;
    targets: ReadonlyMap<string, number>;
    onSaved: (targets: Map<string, number>) => void;
  } = $props();

  /** The plan is complete only when the targets add up to 100%. */
  const SUM_TOLERANCE_PCT = 0.05;

  let saving = $state(false);
  let formError = $state('');

  // Edited copy of the saved targets; reseeded whenever the dialog opens so a
  // cancelled edit never leaks into the next session.
  const draft = new SvelteMap<string, number>();

  useDialogReset(() => open, sync);

  function sync() {
    draft.clear();
    for (const [guid, pct] of targets) draft.set(guid, pct);
    saving = false;
    formError = '';
  }

  let result = $derived(computeRebalance(holdings, draft));

  let dirty = $derived.by(() => {
    if (draft.size !== targets.size) return true;
    for (const [guid, pct] of draft) {
      if (targets.get(guid) !== pct) return true;
    }
    return false;
  });

  // `computeRebalance` sanitizes invalid values, so inspect the draft directly.
  let hasBadValues = $derived(result.rows.some((row) => targetInvalid(row.commodityGuid)));
  let sumValid = $derived(Math.abs(result.targetSumPct - 100) <= SUM_TOLERANCE_PCT);
  let canSave = $derived(dirty && !hasBadValues && sumValid);

  let totalTone = $derived(
    hasBadValues || result.targetSumPct > 100
      ? 'text-destructive'
      : sumValid
        ? 'text-success'
        : result.targetSumPct === 0
          ? 'text-muted-foreground'
          : 'text-warning'
  );

  function rawTarget(commodityGuid: string): number {
    return draft.get(commodityGuid) ?? 0;
  }

  function inputValue(commodityGuid: string): number | '' {
    const raw = rawTarget(commodityGuid);
    return Number.isFinite(raw) ? raw : '';
  }

  function targetInvalid(commodityGuid: string): boolean {
    const raw = rawTarget(commodityGuid);
    return !Number.isFinite(raw) || raw < 0 || raw > 100;
  }

  function setTarget(commodityGuid: string, value: number) {
    formError = '';
    draft.set(commodityGuid, value);
  }

  async function handleSave() {
    saving = true;
    formError = '';
    try {
      const rows = await saveAllocationTargets(
        [...draft].map(([commodityGuid, targetPct]) => ({ commodityGuid, targetPct }))
      );
      onSaved(new Map(rows.map((r) => [r.commodityGuid, r.targetPct])));
      open = false;
      toast.success('Allocation targets saved');
    } catch (e: unknown) {
      formError = errMsg(e, 'Failed to save allocation targets');
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Allocation targets</Dialog.Title>
      <Dialog.Description>
        Set each holding's share of the portfolio. Targets must add up to 100% — nothing is sold,
        cash covers the buys.
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-3 py-4">
      {#each result.rows as row (row.commodityGuid)}
        <label class="flex items-center justify-between gap-4">
          <span class="min-w-0">
            <span class="block truncate text-sm font-medium">{row.fullName ?? row.mnemonic}</span>
            <span class="block truncate text-xs text-muted-foreground">{row.mnemonic}</span>
          </span>
          <span class="relative shrink-0">
            <Input
              type="number"
              min="0"
              max="100"
              step="0.1"
              class="h-8 w-24 pr-7 text-right tabular-nums"
              aria-invalid={targetInvalid(row.commodityGuid)}
              value={inputValue(row.commodityGuid)}
              oninput={(e) => setTarget(row.commodityGuid, e.currentTarget.valueAsNumber)}
            />
            <span
              class="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-muted-foreground"
              >%</span
            >
          </span>
        </label>
      {/each}
    </div>

    <div class="flex flex-col gap-1 text-xs">
      <p class="text-muted-foreground">
        Total
        <span class="{totalTone} font-medium">{formatAllocPct(result.targetSumPct)}</span>
        {#if sumValid && !hasBadValues}
          · Cash to invest
          <span class="font-medium text-foreground"
            >{formatCurrency(result.cashNeeded, currency)}</span
          >
        {/if}
      </p>
      {#if hasBadValues}
        <p class="text-xs text-destructive" role="alert">
          Every target must be between 0 and 100%.
        </p>
      {:else if !sumValid}
        <p class="text-xs text-destructive" role="alert">
          Targets add up to {formatAllocPct(result.targetSumPct)} — they must total 100%.
        </p>
      {/if}
      {#if result.unallocatedCount > 0}
        <p class="text-muted-foreground">
          {plural(result.unallocatedCount, 'holding')} without a target (left out of the calculation).
        </p>
      {/if}
      {#if result.excluded.length > 0}
        <p class="text-muted-foreground">
          {plural(result.excluded.length, 'holding')} excluded (not held or no stored price).
        </p>
      {/if}
      {#if formError}
        <p class="text-sm text-destructive" role="alert">{formError}</p>
      {/if}
    </div>

    <SaveFooter
      mode={targets.size > 0 ? 'edit' : 'create'}
      {saving}
      deleting={false}
      confirmDelete={false}
      saveLabel="Save targets"
      saveDisabled={saving || !canSave}
      onSave={handleSave}
      onCancel={() => (open = false)}
    />
  </Dialog.Content>
</Dialog.Root>
