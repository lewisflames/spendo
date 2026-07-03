<script lang="ts">
  import { CalendarIcon, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import * as Popover from '$lib/components/ui/popover';
  import { Button } from '$lib/components/ui/button';
  import { formatDate } from '$lib/format.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';

  let {
    value = $bindable(''),
    placeholder = 'Pick a date',
    class: className = '',
    firstDayOfWeek = appSettings.value.firstDayOfWeek
  }: {
    value?: string;
    placeholder?: string;
    class?: string;
    firstDayOfWeek?: 0 | 1;
  } = $props();

  let open = $state(false);

  let viewDate = $state(new Date());

  $effect(() => {
    if (!open) return;
    if (value) {
      const d = new Date(value + 'T00:00:00');
      if (!isNaN(d.getTime())) {
        viewDate = new Date(d.getFullYear(), d.getMonth(), 1);
        return;
      }
    }
    viewDate = new Date();
  });
  let viewYear = $derived(viewDate.getFullYear());
  let viewMonth = $derived(viewDate.getMonth());

  function daysInMonth(y: number, m: number): number {
    return new Date(y, m + 1, 0).getDate();
  }

  function firstDayOfMonth(y: number, m: number): number {
    const day = new Date(y, m, 1).getDay();
    return firstDayOfWeek === 1 ? (day + 6) % 7 : day;
  }

  const weekdayLabels = $derived(
    firstDayOfWeek === 1
      ? ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su']
      : ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa']
  );

  const monthNames = [
    'January',
    'February',
    'March',
    'April',
    'May',
    'June',
    'July',
    'August',
    'September',
    'October',
    'November',
    'December'
  ];

  let calendarDays = $derived.by(() => {
    const days: (number | null)[] = [];
    const totalDays = daysInMonth(viewYear, viewMonth);
    const startDay = firstDayOfMonth(viewYear, viewMonth);
    for (let i = 0; i < startDay; i++) days.push(null);
    for (let i = 1; i <= totalDays; i++) days.push(i);
    return days;
  });

  function selectedDay(): number | null {
    if (!value) return null;
    try {
      const d = new Date(value + 'T00:00:00');
      if (d.getFullYear() === viewYear && d.getMonth() === viewMonth) {
        return d.getDate();
      }
    } catch {
      /* ignore */
    }
    return null;
  }

  function displayDate(): string {
    if (!value) return placeholder;
    try {
      return formatDate(value);
    } catch {
      return value;
    }
  }

  function prevMonth() {
    viewDate = new Date(viewYear, viewMonth - 1, 1);
  }

  function nextMonth() {
    viewDate = new Date(viewYear, viewMonth + 1, 1);
  }

  function pickDay(day: number) {
    const y = viewYear;
    const m = String(viewMonth + 1).padStart(2, '0');
    const d = String(day).padStart(2, '0');
    value = `${y}-${m}-${d}`;
    open = false;
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger
    class="border-input dark:bg-input/30 flex h-8 items-center gap-1.5 rounded-lg border bg-transparent px-2.5 py-1 text-sm transition-colors outline-none {className}"
  >
    <CalendarIcon aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
    <span class="text-muted-foreground">{displayDate()}</span>
  </Popover.Trigger>
  <Popover.Content class="w-auto p-3" align="start">
    <div class="flex flex-col gap-2">
      <!-- Month/Year navigation -->
      <div class="flex items-center justify-between">
        <Button variant="ghost" size="sm" class="size-7 p-0" onclick={prevMonth}>
          <ChevronLeft class="size-4" />
        </Button>
        <span class="text-sm font-medium">
          {monthNames[viewMonth]}
          {viewYear}
        </span>
        <Button variant="ghost" size="sm" class="size-7 p-0" onclick={nextMonth}>
          <ChevronRight class="size-4" />
        </Button>
      </div>

      <!-- Day-of-week headers -->
      <div class="grid grid-cols-7 gap-0 text-center text-xs text-muted-foreground">
        {#each weekdayLabels as label (label)}
          <span class="flex h-7 items-center justify-center">{label}</span>
        {/each}
      </div>

      <!-- Day grid -->
      <div class="grid grid-cols-7 gap-0 text-center text-sm">
        {#each calendarDays as day, i (viewYear + '-' + viewMonth + '-' + i)}
          {#if day != null}
            {@const sel = selectedDay()}
            <button
              type="button"
              class="flex h-7 w-7 items-center justify-center rounded-md text-sm transition-colors
								{day === sel
                ? 'bg-primary text-primary-foreground'
                : 'hover:bg-accent hover:text-accent-foreground'}"
              onclick={() => pickDay(day)}
            >
              {day}
            </button>
          {:else}
            <div class="h-7 w-7"></div>
          {/if}
        {/each}
      </div>
    </div>
  </Popover.Content>
</Popover.Root>
