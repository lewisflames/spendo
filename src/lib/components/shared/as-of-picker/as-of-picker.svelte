<script lang="ts">
  import { RotateCcw } from '@lucide/svelte';
  import * as ButtonGroup from '$lib/components/ui/button-group';
  import { Button } from '$lib/components/ui/button';
  import { DatePicker } from '$lib/components/shared/date-picker';
  import { todayStr } from '$lib/utils/date-utils.svelte';

  let { value = $bindable('') }: { value?: string } = $props();

  let isToday = $derived(!value || value === todayStr());

  function resetToToday() {
    value = todayStr();
  }
</script>

<ButtonGroup.Root aria-label="As-of date">
  <DatePicker bind:value />
  <Button
    variant="outline"
    size="icon"
    title="Reset to today"
    aria-label="Reset to today"
    disabled={isToday}
    onclick={resetToToday}
  >
    <RotateCcw aria-hidden="true" class="size-4" />
  </Button>
</ButtonGroup.Root>
