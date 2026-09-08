<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { CalendarCheck, CalendarClock, SkipForward } from '@lucide/svelte';
  import { dbSession, ensureDbSessionLoaded } from '$lib/stores/db.svelte';
  import { bookSettings, loadBookSettings } from '$lib/stores/book-settings.svelte';
  import {
    fetchUpcomingReminders,
    postScheduleOccurrence,
    skipScheduleOccurrence
  } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import {
    formatCurrency,
    formatDate,
    parseLocalDate,
    plural,
    sessionCurrency
  } from '$lib/utils/format.svelte';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { openTransaction } from '$lib/utils/navigation';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  $effect(() => {
    loadBookSettings().catch(() => {});
  });

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => fetchUpcomingReminders(bookSettings.value.remindersLookaheadDays),
    label: 'Failed to load reminders'
  });

  let busyGuid = $state<string | null>(null);

  function dueLabel(nextDueDate: string, overdueDays: number): string {
    if (overdueDays > 0) return `Overdue by ${plural(overdueDays, 'day')}`;
    if (nextDueDate === todayStr()) return 'Due today';
    return `Due ${formatDate(parseLocalDate(nextDueDate), { month: 'short', day: 'numeric' })}`;
  }

  async function refreshAfterMutation() {
    res.reload();
    // Posting changes ledger totals; keep the KPI cards and history honest.
    await ensureDbSessionLoaded(true).catch(() => {});
  }

  async function handlePost(scheduleGuid: string) {
    if (busyGuid) return;
    busyGuid = scheduleGuid;
    try {
      const txGuid = await postScheduleOccurrence(scheduleGuid);
      toast.success('Transaction posted');
      await refreshAfterMutation();
      await openTransaction(txGuid);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to post occurrence'));
    } finally {
      busyGuid = null;
    }
  }

  async function handleSkip(scheduleGuid: string, name: string) {
    if (busyGuid) return;
    busyGuid = scheduleGuid;
    try {
      const next = await skipScheduleOccurrence(scheduleGuid);
      toast.success(
        next ? `Skipped "${name}" — next due ${formatDate(next)}` : `Skipped "${name}"`
      );
      await refreshAfterMutation();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to skip occurrence'));
    } finally {
      busyGuid = null;
    }
  }
</script>

{#if res.loading && !res.data}
  <TableSkeleton cols={3} />
{:else if res.error}
  <ErrorCard message={res.error} onRetry={() => res.reload()} />
{:else if res.data && res.data.length > 0}
  {@const currency = sessionCurrency()}
  <Card.Root>
    <Card.Header>
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <CalendarClock aria-hidden="true" class="size-4 text-muted-foreground" />
          <Card.Title>Upcoming</Card.Title>
        </div>
        <a
          href={resolve('/scheduled')}
          class="text-sm text-muted-foreground underline-offset-4 hover:underline"
        >
          View all
        </a>
      </div>
      <Card.Description>
        Scheduled transactions due within the next {plural(
          bookSettings.value.remindersLookaheadDays,
          'day'
        )}
      </Card.Description>
    </Card.Header>
    <Card.Content class="flex flex-col gap-2">
      {#each res.data as reminder (reminder.scheduleGuid)}
        {@const busy = busyGuid === reminder.scheduleGuid}
        <div
          class="flex items-center justify-between gap-3 rounded-lg border px-3 py-2.5 {reminder.overdueDays >
          0
            ? 'border-destructive/40 bg-destructive/5'
            : ''}"
        >
          <div class="flex min-w-0 flex-col gap-0.5">
            <span class="truncate text-sm font-medium">{reminder.name}</span>
            <span
              class="text-xs tabular-nums {reminder.overdueDays > 0
                ? 'text-destructive'
                : 'text-muted-foreground'}"
            >
              {dueLabel(reminder.nextDueDate, reminder.overdueDays)} ·
              {formatCurrency(reminder.previewAmount, currency)}
            </span>
          </div>
          <div class="flex shrink-0 items-center gap-1">
            {#if reminder.nextDueDate <= todayStr()}
              <Button
                variant="outline"
                size="sm"
                onclick={() => handlePost(reminder.scheduleGuid)}
                disabled={busyGuid !== null}
                title="Post this occurrence to the ledger"
              >
                <CalendarCheck aria-hidden="true" class="size-3.5" />
                {busy ? 'Posting…' : 'Post'}
              </Button>
            {/if}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => handleSkip(reminder.scheduleGuid, reminder.name)}
              disabled={busyGuid !== null}
              title="Skip this occurrence without posting"
            >
              <SkipForward aria-hidden="true" class="size-3.5" />
              Skip
            </Button>
          </div>
        </div>
      {/each}
    </Card.Content>
  </Card.Root>
{/if}
