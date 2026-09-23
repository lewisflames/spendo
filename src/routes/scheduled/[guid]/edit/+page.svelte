<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { toast } from '$lib/components/ui/sonner';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchSchedule, type Schedule } from '$lib/tauri';
  import ScheduleForm from '$lib/features/schedules/schedule-form.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  let guid = $derived(page.params.guid);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    fetch: () => {
      const id = guid;
      if (!id) return Promise.reject(new Error('No schedule selected'));
      return fetchSchedule(id);
    },
    label: 'Failed to load schedule'
  });

  let schedule = $derived(res.data ?? null);

  function goBack() {
    void goto(resolve('/scheduled'), { replaceState: true });
  }

  function handleSaved(updated: Schedule) {
    toast.success(`Updated "${updated.name}"`);
    goBack();
  }

  function handleDeleted() {
    toast.success('Schedule deleted');
    void goto(resolve('/scheduled'));
  }
</script>

<svelte:head>
  <title>{schedule ? `Edit ${schedule.name} — Spendo` : 'Edit schedule — Spendo'}</title>
  <meta name="description" content="Edit a recurring schedule and its template splits." />
</svelte:head>

<BookGate title="Edit schedule" description="Edit a recurring schedule and its template splits">
  <div class="mx-auto flex w-full max-w-5xl flex-col gap-6">
    {#if (res.loading || dbBootstrapping.value) && !res.data}
      <PageSkeleton variant="form" />
    {:else if schedule}
      {#key schedule.guid}
        <ScheduleForm
          mode="edit"
          guid={schedule.guid}
          title="Edit schedule"
          headerDescription={schedule.name}
          initial={schedule}
          onSaved={handleSaved}
          onDeleted={handleDeleted}
          onCancel={goBack}
          onError={(msg) => toast.error(msg)}
        />
      {/key}
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
