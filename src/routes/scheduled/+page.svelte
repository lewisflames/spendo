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
  import ScheduleEditorDialog from '$lib/features/schedules/schedule-editor-dialog.svelte';
  import ScheduleTable from '$lib/features/schedules/schedule-table.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Plus, CalendarClock } from '@lucide/svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { openTransaction } from '$lib/utils/navigation';
  import { formatDate, plural, sessionCurrency } from '$lib/utils/format.svelte';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { errMsg } from '$lib/utils/error';

  let currency = $derived(sessionCurrency());
  let today = $derived(todayStr());

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchSchedules,
    label: 'Failed to load scheduled transactions'
  });

  let dialogOpen = $state(false);
  let dialogMode = $state<'create' | 'edit'>('create');
  let dialogGuid = $state<string | null>(null);
  let dialogInitial = $state<Schedule | null>(null);
  let busyGuid = $state<string | null>(null);

  function openCreate() {
    dialogMode = 'create';
    dialogGuid = null;
    dialogInitial = null;
    dialogOpen = true;
  }

  function openEdit(schedule: Schedule) {
    dialogMode = 'edit';
    dialogGuid = schedule.guid;
    dialogInitial = schedule;
    dialogOpen = true;
  }

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
      toast.success(`Posted "${schedule.name}"`);
      await refreshAfterMutation();
      await openTransaction(txGuid);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to post occurrence'));
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
          ? `Skipped "${schedule.name}" — next due ${formatDate(next)}`
          : `Skipped "${schedule.name}"`
      );
      await refreshAfterMutation();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to skip occurrence'));
    } finally {
      busyGuid = null;
    }
  }

  async function handleToggleActive(schedule: Schedule) {
    if (busyGuid) return;
    busyGuid = schedule.guid;
    try {
      await setScheduleActive(schedule.guid, !schedule.active);
      toast.success(schedule.active ? `Paused "${schedule.name}"` : `Resumed "${schedule.name}"`);
      res.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to update schedule'));
    } finally {
      busyGuid = null;
    }
  }
</script>

<svelte:head>
  <title>Scheduled — Spendo</title>
  <meta
    name="description"
    content="Recurring transactions with manual reminders. Nothing posts automatically."
  />
</svelte:head>

<BookGate title="Scheduled" description="Recurring transactions and reminders">
  <div class="flex flex-col gap-6">
    <PageHeader title="Scheduled" description={res.data ? plural(res.data.length, 'schedule') : ''}>
      {#if dbSession.value}
        <Button onclick={openCreate}>
          <Plus aria-hidden="true" class="size-4" />
          New schedule
        </Button>
      {/if}
    </PageHeader>

    <ScheduleEditorDialog
      bind:open={dialogOpen}
      mode={dialogMode}
      guid={dialogGuid}
      initial={dialogInitial}
      onSaved={(s) => {
        res.reload();
        toast.success(dialogMode === 'edit' ? `Updated "${s.name}"` : `Created "${s.name}"`);
      }}
      onDeleted={() => {
        res.reload();
        toast.success('Schedule deleted');
      }}
      onError={(msg) => toast.error(msg)}
    />

    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="scheduled" />
    {:else if res.data}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}
      {#if res.data.length === 0}
        <EmptyState
          card
          title="No scheduled transactions"
          description="Set up rent, subscriptions or paychecks — due occurrences surface as reminders to post or skip. Nothing posts automatically."
        >
          {#snippet icon()}
            <CalendarClock aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
          {#snippet action()}
            <Button onclick={openCreate}>
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
            onEdit={openEdit}
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
