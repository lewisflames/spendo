<script lang="ts">
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import {
    fetchSchedules,
    postScheduleOccurrence,
    setScheduleActive,
    skipScheduleOccurrence,
    type Schedule
  } from '$lib/tauri';
  import { dbBootstrapping, dbSession, ensureDbSessionLoaded } from '$lib/stores/db.svelte';
  import ScheduleTable from '$lib/features/schedules/schedule-table.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Plus, CalendarClock } from '@lucide/svelte';
  import { resolve } from '$app/paths';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { openTransaction } from '$lib/utils/navigation';
  import { formatDate, plural, sessionCurrency } from '$lib/utils/format.svelte';
  import { today as sharedToday } from '$lib/utils/date-utils.svelte';
  import { errMsg } from '$lib/utils/error';

  let currency = $derived(sessionCurrency());
  let today = $derived(sharedToday.value);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchSchedules,
    label: 'We couldn’t load your schedules'
  });

  let busyGuid = $state<string | null>(null);

  async function refreshAfterMutation() {
    res.reload();
    // Posting changes ledger totals; keep the session KPIs honest.
    await ensureDbSessionLoaded(true).catch(() => {});
  }

  async function handlePost(schedule: Schedule) {
    if (busyGuid) return;
    busyGuid = schedule.guid;
    try {
      const txGuid = await postScheduleOccurrence(schedule.guid);
      toast.success(`Added “${schedule.name}” to your ledger`);
      await refreshAfterMutation();
      await openTransaction(txGuid);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t post that occurrence'));
    } finally {
      busyGuid = null;
    }
  }

  async function handleSkip(schedule: Schedule) {
    if (busyGuid) return;
    busyGuid = schedule.guid;
    try {
      const next = await skipScheduleOccurrence(schedule.guid);
      toast.success(
        next
          ? `Skipped “${schedule.name}”. Next due ${formatDate(next)}`
          : `Skipped “${schedule.name}”`
      );
      await refreshAfterMutation();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t skip that occurrence'));
    } finally {
      busyGuid = null;
    }
  }

  async function handleToggleActive(schedule: Schedule) {
    if (busyGuid) return;
    busyGuid = schedule.guid;
    try {
      await setScheduleActive(schedule.guid, !schedule.active);
      toast.success(schedule.active ? `Paused “${schedule.name}”` : `Resumed “${schedule.name}”`);
      res.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t update that schedule'));
    } finally {
      busyGuid = null;
    }
  }
</script>

<svelte:head>
  <title>Scheduled · Spendo</title>
  <meta
    name="description"
    content="Bills and paychecks you revisit. You stay in control. Nothing posts on its own."
  />
</svelte:head>

<BookGate title="Scheduled" description="Bills and paychecks, on repeat">
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Scheduled"
      description={res.data
        ? `Bills and paychecks on repeat · ${plural(res.data.length, 'schedule')}`
        : 'Bills and paychecks, on repeat'}
    >
      {#if dbSession.value}
        <Button href={resolve('/scheduled/new')}>
          <Plus aria-hidden="true" class="size-4" />
          New schedule
        </Button>
      {/if}
    </PageHeader>

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="scheduled" />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if res.data.length === 0}
        <EmptyState
          card
          title="Nothing on repeat yet"
          description="Set up rent, subscriptions or paydays. We’ll nudge you when one’s due. Nothing posts without you."
        >
          {#snippet icon()}
            <CalendarClock aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
          {#snippet action()}
            <Button href={resolve('/scheduled/new')}>
              <Plus aria-hidden="true" class="size-4" />
              New schedule
            </Button>
          {/snippet}
        </EmptyState>
      {:else}
        <DimWrapper active={res.loading} class="flex flex-col gap-4">
          <ScheduleTable
            schedules={res.data}
            {currency}
            {today}
            busy={busyGuid !== null}
            onPost={handlePost}
            onSkip={handleSkip}
            onToggleActive={handleToggleActive}
          />
        </DimWrapper>
      {/if}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
