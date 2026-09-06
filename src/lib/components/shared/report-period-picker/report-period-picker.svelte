<script lang="ts">
  import { untrack } from 'svelte';
  import type { CalendarDate } from '@internationalized/date';
  import * as Popover from '$lib/components/ui/popover';
  import { Button } from '$lib/components/ui/button';
  import { CalendarIcon, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import type { DateRange } from 'bits-ui';
  import type { DatePreset } from '$lib/utils/date-utils.svelte';
  import { getMonthRange, getMonthRangeSpan } from '$lib/utils/date-utils.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import {
    formatPeriodLabel,
    getMonthShortNames,
    getReportYearBounds,
    isAtBookStart,
    isAtPresent,
    isSameRange,
    shiftPeriodRange
  } from '$lib/features/reports/report-period';

  let {
    value = $bindable<DateRange>(),
    presets = [] as DatePreset[],
    defaultLabel = 'Select period'
  }: {
    value: DateRange;
    presets?: DatePreset[];
    defaultLabel?: string;
  } = $props();

  let open = $state(false);
  let visibleYear = $state<number>(new Date().getFullYear());
  // First click of a two-click month-range pick; survives year stepping so
  // ranges can span years. Hover previews the pending span.
  let anchor = $state<{ y: number; m: number } | null>(null);
  let hover = $state<{ y: number; m: number } | null>(null);

  let summary = $derived(dbSession.value?.summary ?? null);
  let bounds = $derived(
    getReportYearBounds(summary?.earliestDate ?? null, summary?.latestDate ?? null)
  );
  let minYear = $derived(bounds.minYear);
  let maxYear = $derived(bounds.maxYear);
  let monthNames = $derived(getMonthShortNames(appSettings.value.locale));

  let display = $derived(
    formatPeriodLabel(
      value?.start as CalendarDate | undefined,
      value?.end as CalendarDate | undefined
    ) ?? defaultLabel
  );

  // Keep the year stepper on the committed value whenever the popover opens.
  // A fresh open also clears any pending month-range pick.
  $effect(() => {
    if (open) {
      untrack(() => {
        const endYear = (value?.end as CalendarDate | undefined)?.year ?? new Date().getFullYear();
        visibleYear = Math.min(maxYear, Math.max(minYear, endYear));
        anchor = null;
        hover = null;
      });
    } else {
      anchor = null;
      hover = null;
    }
  });

  function applyPreset(preset: DatePreset) {
    value = preset.getRange();
    anchor = null;
    hover = null;
    open = false;
  }

  function presetActive(preset: DatePreset): boolean {
    if (!value?.start || !value?.end) return false;
    try {
      return isSameRange(preset.getRange(), {
        start: value.start as CalendarDate,
        end: value.end as CalendarDate
      });
    } catch {
      return false;
    }
  }

  function stepPeriod(direction: 1 | -1) {
    const start = value?.start as CalendarDate | undefined;
    const end = value?.end as CalendarDate | undefined;
    if (!start || !end) return;
    value = shiftPeriodRange(start, end, direction);
    anchor = null;
    hover = null;
  }

  let canStepPrev = $derived.by(() => {
    const start = value?.start as CalendarDate | undefined;
    if (!start || !value?.end) return false;
    return !isAtBookStart(start, summary?.earliestDate ?? null);
  });

  let canStepNext = $derived.by(() => {
    const start = value?.start as CalendarDate | undefined;
    const end = value?.end as CalendarDate | undefined;
    if (!start || !end) return false;
    return !isAtPresent(start, end);
  });

  function absMonth(y: number, m: number): number {
    return y * 12 + (m - 1);
  }

  function clickMonth(y: number, m: number) {
    // Two-click month-range pick: first click anchors, second click commits
    // the whole-month span (same month twice = single month).
    if (!anchor) {
      anchor = { y, m };
      hover = { y, m };
      return;
    }
    value = getMonthRangeSpan(anchor.y, anchor.m, y, m);
    anchor = null;
    hover = null;
    open = false;
  }

  function monthInPendingSpan(y: number, m: number): boolean {
    if (!anchor) return false;
    const other = hover ?? anchor;
    const lo = Math.min(absMonth(anchor.y, anchor.m), absMonth(other.y, other.m));
    const hi = Math.max(absMonth(anchor.y, anchor.m), absMonth(other.y, other.m));
    const cur = absMonth(y, m);
    return cur >= lo && cur <= hi;
  }

  function monthSelected(y: number, m: number): boolean {
    if (monthInPendingSpan(y, m)) return true;
    const start = value?.start as CalendarDate | undefined;
    const end = value?.end as CalendarDate | undefined;
    if (!start || !end) return false;
    const ms = getMonthRange(y, m).start;
    const me = getMonthRange(y, m).end;
    return ms.compare(end) <= 0 && me.compare(start) >= 0;
  }

  function stepYear(delta: number) {
    visibleYear = Math.min(maxYear, Math.max(minYear, visibleYear + delta));
  }
</script>

<Popover.Root bind:open>
  <div class="flex" role="group" aria-label="Report period">
    <Button
      variant="outline"
      size="icon"
      class="rounded-r-none"
      aria-label="Previous period"
      disabled={!canStepPrev}
      onclick={() => stepPeriod(-1)}
    >
      <ChevronLeft aria-hidden="true" />
    </Button>
    <Popover.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" class="justify-between rounded-none border-x-0">
          <CalendarIcon aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
          {display}
        </Button>
      {/snippet}
    </Popover.Trigger>
    <Button
      variant="outline"
      size="icon"
      class="rounded-l-none"
      aria-label="Next period"
      disabled={!canStepNext}
      onclick={() => stepPeriod(1)}
    >
      <ChevronRight aria-hidden="true" />
    </Button>
  </div>
  <Popover.Content align="end" class="w-85">
    <div class="flex flex-wrap gap-1.5">
      {#each presets as preset (preset.label)}
        <Button
          size="sm"
          variant={presetActive(preset) ? 'secondary' : 'outline'}
          aria-pressed={presetActive(preset)}
          onclick={() => applyPreset(preset)}
        >
          {preset.label}
        </Button>
      {/each}
    </div>

    <div class="flex items-center justify-between">
      <Button
        variant="outline"
        size="icon-sm"
        onclick={() => stepYear(-1)}
        disabled={visibleYear <= minYear}
        aria-label="Previous year"
      >
        <ChevronLeft aria-hidden="true" />
      </Button>
      <span class="min-w-16 text-center text-sm font-medium">{visibleYear}</span>
      <Button
        variant="outline"
        size="icon-sm"
        onclick={() => stepYear(1)}
        disabled={visibleYear >= maxYear}
        aria-label="Next year"
      >
        <ChevronRight aria-hidden="true" />
      </Button>
    </div>
    <div
      class="grid grid-cols-3 gap-1"
      role="group"
      aria-label="Choose a month range"
      onmouseleave={() => {
        hover = anchor;
      }}
    >
      {#each monthNames as name, i (name + i)}
        <Button
          size="sm"
          variant={monthSelected(visibleYear, i + 1) ? 'secondary' : 'ghost'}
          aria-pressed={monthSelected(visibleYear, i + 1)}
          onclick={() => clickMonth(visibleYear, i + 1)}
          onmouseenter={() => {
            hover = { y: visibleYear, m: i + 1 };
          }}
          onfocus={() => {
            hover = { y: visibleYear, m: i + 1 };
          }}
        >
          {name}
        </Button>
      {/each}
    </div>
    <p class="text-xs text-muted-foreground">
      {anchor ? 'Select end month' : 'Select start month, then end month'}
    </p>
  </Popover.Content>
</Popover.Root>
