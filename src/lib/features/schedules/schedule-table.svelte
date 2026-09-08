<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { CalendarCheck, SkipForward, Pencil, Pause, Play } from '@lucide/svelte';
  import type { Schedule } from '$lib/tauri';
  import { formatCurrency, formatDate, parseLocalDate, plural } from '$lib/utils/format.svelte';
  import {
    frequencyLabel,
    isPostable,
    occurrenceLabel,
    scheduleStatus,
    type ScheduleStatus
  } from './schedule-utils';

  let {
    schedules,
    currency,
    today,
    busy = false,
    onEdit,
    onPost,
    onSkip,
    onToggleActive
  }: {
    schedules: Schedule[];
    currency: string;
    /** YYYY-MM-DD for due comparisons. */
    today: string;
    busy?: boolean;
    onEdit: (schedule: Schedule) => void;
    onPost: (schedule: Schedule) => void;
    onSkip: (schedule: Schedule) => void;
    onToggleActive: (schedule: Schedule) => void;
  } = $props();

  const STATUS_BADGE: Record<
    ScheduleStatus,
    { variant: 'default' | 'secondary' | 'destructive' | 'outline'; label: string }
  > = {
    overdue: { variant: 'destructive', label: 'Overdue' },
    upcoming: { variant: 'default', label: 'Scheduled' },
    paused: { variant: 'outline', label: 'Paused' },
    completed: { variant: 'secondary', label: 'Completed' }
  };

  function overdueLabel(nextDueDate: string): string {
    const days = Math.round(
      (parseLocalDate(today).getTime() - parseLocalDate(nextDueDate).getTime()) / 86_400_000
    );
    if (days <= 0) return 'Due today';
    return `Overdue by ${plural(days, 'day')}`;
  }
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head>Status</Table.Head>
      <Table.Head>Name</Table.Head>
      <Table.Head>Repeats</Table.Head>
      <Table.Head>Next due</Table.Head>
      <Table.Head class="text-right">Amount</Table.Head>
      <Table.Head>Posted</Table.Head>
      <Table.Head class="w-56 text-right">Action</Table.Head>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each schedules as schedule (schedule.guid)}
      {@const status = scheduleStatus(schedule, today)}
      {@const badge = STATUS_BADGE[status]}
      <Table.Row class="hover:bg-muted/50">
        <Table.Cell class="py-2.5 whitespace-nowrap">
          <Badge variant={badge.variant}>{badge.label}</Badge>
        </Table.Cell>
        <Table.Cell class="py-2.5">
          <span class="font-medium">{schedule.name}</span>
          {#if schedule.description && schedule.description !== schedule.name}
            <span class="block text-xs text-muted-foreground">{schedule.description}</span>
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap">
          {frequencyLabel(schedule.frequency, schedule.intervalMult)}
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap">
          {#if schedule.nextDueDate}
            <span class="tabular-nums">{formatDate(schedule.nextDueDate)}</span>
            {#if status === 'overdue'}
              <span class="block text-xs text-destructive tabular-nums">
                {overdueLabel(schedule.nextDueDate)}
              </span>
            {/if}
          {:else}
            <span class="text-xs text-muted-foreground">—</span>
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right tabular-nums">
          {formatCurrency(schedule.previewAmount, currency)}
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap tabular-nums">
          {occurrenceLabel(schedule)}
        </Table.Cell>
        <Table.Cell class="py-2.5 text-right">
          <div class="flex items-center justify-end gap-1">
            {#if schedule.nextDueDate && schedule.active}
              {#if isPostable(schedule.nextDueDate, today)}
                <Button
                  variant="outline"
                  size="sm"
                  onclick={() => onPost(schedule)}
                  disabled={busy}
                  title="Post this occurrence to the ledger"
                >
                  <CalendarCheck aria-hidden="true" class="size-3.5" />
                  Post
                </Button>
              {/if}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => onSkip(schedule)}
                disabled={busy}
                title="Skip this occurrence without posting"
              >
                <SkipForward aria-hidden="true" class="size-3.5" />
                Skip
              </Button>
            {/if}
            {#if status !== 'completed'}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => onToggleActive(schedule)}
                disabled={busy}
                title={schedule.active ? 'Pause reminders' : 'Resume reminders'}
              >
                {#if schedule.active}
                  <Pause aria-hidden="true" class="size-3.5" />
                  Pause
                {:else}
                  <Play aria-hidden="true" class="size-3.5" />
                  Resume
                {/if}
              </Button>
            {/if}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => onEdit(schedule)}
              disabled={busy}
              title="Edit schedule"
            >
              <Pencil aria-hidden="true" class="size-3.5" />
              Edit
            </Button>
          </div>
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
