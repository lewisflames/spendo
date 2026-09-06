<script lang="ts">
  import { parseDate, type DateValue } from '@internationalized/date';
  import { CalendarIcon } from '@lucide/svelte';
  import * as Popover from '$lib/components/ui/popover';
  import { Button } from '$lib/components/ui/button';
  import { Calendar } from '$lib/components/ui/calendar';
  import { formatDate } from '$lib/utils/format.svelte';
  import { appSettings } from '$lib/stores/settings.svelte';

  let {
    value = $bindable(''),
    placeholder = 'Pick a date',
    id,
    class: className = ''
  }: {
    value?: string;
    placeholder?: string;
    id?: string;
    class?: string;
  } = $props();

  let open = $state(false);

  let calendarValue = $derived.by(() => {
    if (!value) return undefined;
    try {
      return parseDate(value);
    } catch {
      return undefined;
    }
  });

  // Week start follows the configured locale.
  let locale = $derived(appSettings.value.locale);
  let display = $derived(value ? formatDate(value) : placeholder);

  function onValueChange(d: DateValue | undefined) {
    value = d ? d.toString() : '';
    if (d) open = false;
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger {id} class={className}>
    {#snippet child({ props })}
      <Button {...props} type="button" variant="outline">
        <CalendarIcon aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
        <span class="text-muted-foreground">{display}</span>
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content class="w-auto p-0" align="start">
    <Calendar
      type="single"
      value={calendarValue}
      {onValueChange}
      {locale}
      class="bg-transparent p-2 [--cell-size:--spacing(7)]"
    />
  </Popover.Content>
</Popover.Root>
