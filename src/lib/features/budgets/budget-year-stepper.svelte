<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import { ChevronLeft, ChevronRight } from '@lucide/svelte';
  import type { YearOption } from './budget-utils';

  let {
    yearSel = $bindable(''),
    yearOptions,
    atMin,
    atMax,
    selectId = 'year-picker',
    onPrev,
    onNext
  }: {
    yearSel: string;
    yearOptions: YearOption[];
    atMin: boolean;
    atMax: boolean;
    selectId?: string;
    onPrev: () => void;
    onNext: () => void;
  } = $props();
</script>

<div class="flex items-center gap-2">
  <ButtonGroup.Root>
    <Button
      variant="outline"
      size="icon"
      onclick={onPrev}
      aria-label="Previous year"
      disabled={atMin}
    >
      <ChevronLeft />
    </Button>
    <Select.Root type="single" bind:value={yearSel} items={yearOptions}>
      <Select.Trigger class="min-w-20 justify-center" id={selectId}>
        <SelectPrimitive.Value />
      </Select.Trigger>
      <Select.Content>
        {#each yearOptions as o (o.value)}
          <Select.Item value={o.value} label={o.label}>{o.label}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
    <Button variant="outline" size="icon" onclick={onNext} aria-label="Next year" disabled={atMax}>
      <ChevronRight />
    </Button>
  </ButtonGroup.Root>
</div>
