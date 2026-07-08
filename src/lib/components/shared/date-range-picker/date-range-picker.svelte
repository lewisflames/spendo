<script lang="ts">
  import { getLocalTimeZone } from '@internationalized/date';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import * as Popover from '$lib/components/ui/popover';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
  import { Button } from '$lib/components/ui/button';
  import { RangeCalendar } from '$lib/components/ui/range-calendar';
  import { formatDate } from '$lib/utils/format.svelte';
  import { CalendarIcon, ChevronDown } from '@lucide/svelte';
  import type { DateRange } from 'bits-ui';
  import type { DatePreset } from '$lib/utils/date-utils.svelte';

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

  let display = $derived(
    value?.start && value?.end
      ? `${formatDate(value.start.toDate(getLocalTimeZone()))} – ${formatDate(value.end.toDate(getLocalTimeZone()))}`
      : defaultLabel
  );

  function applyPreset(preset: DatePreset) {
    value = preset.getRange();
    open = false;
  }

  function onValueChange() {
    if (value?.start && value?.end) {
      open = false;
    }
  }
</script>

<ButtonGroup.Root>
  <Popover.Root bind:open>
    <Popover.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" class="justify-between">
          <CalendarIcon aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
          {display}
        </Button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content align="end" class="overflow-hidden">
      <RangeCalendar
        bind:value
        {onValueChange}
        class="bg-transparent p-0 [--cell-size:--spacing(9.5)]"
      />
    </Popover.Content>
  </Popover.Root>
  {#if presets.length > 0}
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="outline" class="ps-2!">
            <ChevronDown aria-hidden="true" />
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end">
        {#each presets as preset (preset.label)}
          <DropdownMenu.Item onclick={() => applyPreset(preset)}>
            {preset.label}
          </DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  {/if}
</ButtonGroup.Root>
