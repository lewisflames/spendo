<script lang="ts">
  import { untrack } from 'svelte';
  import { parseDate, type CalendarDate } from '@internationalized/date';
  import * as Popover from '$lib/components/ui/popover';
  import { Button } from '$lib/components/ui/button';
  import { CalendarIcon, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import type { DateRange } from 'bits-ui';
  import type { DatePreset } from '$lib/utils/date-utils.svelte';
  import { getMonthRange, getQuarterRange } from '$lib/utils/date-utils.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';
  import {
    formatPeriodLabel,
    fromAbsMonth,
    fromAbsQuarter,
    getMonthShortNames,
    getReportYearBounds,
    inferPeriodTab,
    isSameRange,
    rangeFromAbsMonths,
    rangeFromAbsQuarters,
    rangeFromYears,
    toAbsMonth,
    toAbsQuarter,
    type PeriodTab
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
  let tab = $state<PeriodTab>('month');
  let draft = $state<DateRange>(value);
  let anchorYear = $state<number | null>(null);
  let anchorQuarter = $state<number | null>(null);
  let anchorMonth = $state<number | null>(null);
  let visibleYear = $state<number>(new Date().getFullYear());

  let summary = $derived(dbSession.value?.summary ?? null);
  let bounds = $derived(
    getReportYearBounds(summary?.earliestDate ?? null, summary?.latestDate ?? null)
  );
  let minYear = $derived(bounds.minYear);
  let maxYear = $derived(bounds.maxYear);
  let yearList = $derived(Array.from({ length: maxYear - minYear + 1 }, (_, i) => maxYear - i));
  let monthNames = $derived(getMonthShortNames(appSettings.value.locale));

  let allTimeRange = $derived.by(() => {
    if (!summary?.earliestDate || !summary?.latestDate) return null;
    try {
      return {
        start: parseDate(summary.earliestDate.slice(0, 10)),
        end: parseDate(summary.latestDate.slice(0, 10))
      };
    } catch {
      return null;
    }
  });

  let display = $derived(
    formatPeriodLabel(
      value?.start as CalendarDate | undefined,
      value?.end as CalendarDate | undefined
    ) ?? defaultLabel
  );

  let draftLabel = $derived(
    formatPeriodLabel(
      draft?.start as CalendarDate | undefined,
      draft?.end as CalendarDate | undefined
    ) ?? 'Choose a period'
  );

  // Reset the click-only draft whenever the popover opens so Cancel never
  // leaks a half-built range and the tab matches the committed value.
  $effect(() => {
    if (open) {
      untrack(() => {
        if (value?.start && value?.end) {
          draft = { start: value.start, end: value.end };
        }
        tab = inferPeriodTab(
          value?.start as CalendarDate | undefined,
          value?.end as CalendarDate | undefined
        );
        const endYear = (value?.end as CalendarDate | undefined)?.year ?? new Date().getFullYear();
        visibleYear = Math.min(maxYear, Math.max(minYear, endYear));
        anchorYear = null;
        anchorQuarter = null;
        anchorMonth = null;
      });
    }
  });

  function applyPreset(preset: DatePreset) {
    value = preset.getRange();
    open = false;
  }

  function applyAllTime() {
    if (!allTimeRange) return;
    value = allTimeRange;
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

  let allTimeActive = $derived(
    !!allTimeRange &&
      !!value?.start &&
      !!value?.end &&
      isSameRange(allTimeRange, {
        start: value.start as CalendarDate,
        end: value.end as CalendarDate
      })
  );

  function clickYear(y: number) {
    if (anchorYear === null) {
      anchorYear = y;
      draft = rangeFromYears(y, y);
    } else {
      draft = rangeFromYears(anchorYear, y);
      anchorYear = null;
    }
  }

  function clickQuarter(y: number, q: number) {
    const abs = toAbsQuarter(y, q);
    if (anchorQuarter === null) {
      anchorQuarter = abs;
      draft = rangeFromAbsQuarters(abs, abs);
    } else {
      draft = rangeFromAbsQuarters(anchorQuarter, abs);
      anchorQuarter = null;
    }
  }

  function clickMonth(y: number, m: number) {
    const abs = toAbsMonth(y, m);
    if (anchorMonth === null) {
      anchorMonth = abs;
      draft = rangeFromAbsMonths(abs, abs);
    } else {
      draft = rangeFromAbsMonths(anchorMonth, abs);
      anchorMonth = null;
    }
  }

  function yearSelected(y: number): boolean {
    const start = draft?.start as CalendarDate | undefined;
    const end = draft?.end as CalendarDate | undefined;
    if (!start || !end) return false;
    return y >= start.year && y <= end.year;
  }

  function quarterSelected(y: number, q: number): boolean {
    const start = draft?.start as CalendarDate | undefined;
    const end = draft?.end as CalendarDate | undefined;
    if (!start || !end) return false;
    const qs = getQuarterRange(y, q).start;
    const qe = getQuarterRange(y, q).end;
    return qs.compare(end) <= 0 && qe.compare(start) >= 0;
  }

  function monthSelected(y: number, m: number): boolean {
    const start = draft?.start as CalendarDate | undefined;
    const end = draft?.end as CalendarDate | undefined;
    if (!start || !end) return false;
    const ms = getMonthRange(y, m).start;
    const me = getMonthRange(y, m).end;
    return ms.compare(end) <= 0 && me.compare(start) >= 0;
  }

  function quarterPressed(y: number, q: number): boolean {
    if (anchorQuarter === null) return quarterSelected(y, q);
    const current = fromAbsQuarter(anchorQuarter);
    return current.year === y && current.quarter === q;
  }

  function monthPressed(y: number, m: number): boolean {
    if (anchorMonth === null) return monthSelected(y, m);
    const current = fromAbsMonth(anchorMonth);
    return current.year === y && current.month === m;
  }

  function stepYear(delta: number) {
    visibleYear = Math.min(maxYear, Math.max(minYear, visibleYear + delta));
  }

  function applyDraft() {
    if (!draft?.start || !draft?.end) return;
    value = { start: draft.start, end: draft.end };
    open = false;
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      <Button {...props} variant="outline" class="justify-between">
        <CalendarIcon aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
        {display}
      </Button>
    {/snippet}
  </Popover.Trigger>
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
      {#if allTimeRange}
        <Button
          size="sm"
          variant={allTimeActive ? 'secondary' : 'outline'}
          aria-pressed={allTimeActive}
          onclick={applyAllTime}
        >
          All time
        </Button>
      {/if}
    </div>

    <div
      role="tablist"
      aria-label="Period granularity"
      class="inline-flex items-center gap-1 self-start rounded-lg bg-muted p-1"
    >
      <Button
        variant={tab === 'year' ? 'secondary' : 'ghost'}
        size="sm"
        role="tab"
        aria-selected={tab === 'year'}
        onclick={() => (tab = 'year')}
      >
        Year
      </Button>
      <Button
        variant={tab === 'quarter' ? 'secondary' : 'ghost'}
        size="sm"
        role="tab"
        aria-selected={tab === 'quarter'}
        onclick={() => (tab = 'quarter')}
      >
        Quarter
      </Button>
      <Button
        variant={tab === 'month' ? 'secondary' : 'ghost'}
        size="sm"
        role="tab"
        aria-selected={tab === 'month'}
        onclick={() => (tab = 'month')}
      >
        Month
      </Button>
    </div>

    {#if tab === 'year'}
      <div
        class="grid max-h-56 grid-cols-4 gap-1 overflow-y-auto"
        role="group"
        aria-label="Choose year range"
      >
        {#each yearList as y (y)}
          <Button
            size="sm"
            variant={yearSelected(y) ? 'secondary' : 'ghost'}
            aria-pressed={anchorYear === y || yearSelected(y)}
            onclick={() => clickYear(y)}
          >
            {y}
          </Button>
        {/each}
      </div>
      <p class="text-xs text-muted-foreground">
        Click a year, then a second year for a multi-year range.
      </p>
    {:else if tab === 'quarter'}
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
      <div class="grid grid-cols-4 gap-1" role="group" aria-label="Choose quarter range">
        {#each [1, 2, 3, 4] as q (q)}
          <Button
            size="sm"
            variant={quarterSelected(visibleYear, q) ? 'secondary' : 'ghost'}
            aria-pressed={quarterPressed(visibleYear, q)}
            onclick={() => clickQuarter(visibleYear, q)}
          >
            Q{q}
          </Button>
        {/each}
      </div>
      <p class="text-xs text-muted-foreground">
        Click a quarter, change year if needed, then click a second quarter.
      </p>
    {:else}
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
      <div class="grid grid-cols-3 gap-1" role="group" aria-label="Choose month range">
        {#each monthNames as name, i (name + i)}
          <Button
            size="sm"
            variant={monthSelected(visibleYear, i + 1) ? 'secondary' : 'ghost'}
            aria-pressed={monthPressed(visibleYear, i + 1)}
            onclick={() => clickMonth(visibleYear, i + 1)}
          >
            {name}
          </Button>
        {/each}
      </div>
      <p class="text-xs text-muted-foreground">
        Click a month, change year if needed, then click a second month.
      </p>
    {/if}

    <div class="flex items-center justify-between gap-2 border-t pt-2.5">
      <p class="truncate text-xs text-muted-foreground" title={draftLabel}>{draftLabel}</p>
      <div class="flex shrink-0 gap-1.5">
        <Button variant="ghost" size="sm" onclick={() => (open = false)}>Cancel</Button>
        <Button size="sm" onclick={applyDraft} disabled={!draft?.start || !draft?.end}>
          Apply
        </Button>
      </div>
    </div>
  </Popover.Content>
</Popover.Root>
