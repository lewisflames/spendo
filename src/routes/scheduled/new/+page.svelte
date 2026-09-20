<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { ArrowLeft } from '@lucide/svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Button } from '$lib/components/ui/button';
  import ScheduleForm from '$lib/features/schedules/schedule-form.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import type { Schedule } from '$lib/tauri';

  function handleSaved(schedule: Schedule) {
    toast.success(`Created "${schedule.name}"`);
    void goto(resolve('/scheduled'), { replaceState: true });
  }
</script>

<svelte:head>
  <title>New schedule — Spendo</title>
  <meta name="description" content="Create a recurring schedule with template splits." />
</svelte:head>

<BookGate title="New schedule" description="Create a recurring schedule with template splits">
  <div class="mx-auto flex w-full max-w-6xl flex-col gap-6">
    <PageHeader
      title="New schedule"
      description="Nothing posts automatically — each due occurrence surfaces as a reminder to post or skip."
    >
      <Button variant="outline" href={resolve('/scheduled')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Scheduled
      </Button>
    </PageHeader>
    <ScheduleForm
      mode="create"
      onSaved={handleSaved}
      onCancel={() => void goto(resolve('/scheduled'))}
      onError={(msg) => toast.error(msg)}
    />
  </div>
</BookGate>
