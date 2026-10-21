<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { toast } from '$lib/components/ui/sonner';
  import ScheduleForm from '$lib/features/schedules/schedule-form.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import type { Schedule } from '$lib/tauri';

  function handleSaved(schedule: Schedule) {
    toast.success(`Schedule “${schedule.name}” created`);
    void goto(resolve('/scheduled'), { replaceState: true });
  }
</script>

<svelte:head>
  <title>New schedule · Spendo</title>
  <meta name="description" content="Set up a recurring transaction" />
</svelte:head>

<BookGate title="New schedule" description="Set up a recurring transaction">
  <div class="page-form page-stack">
    <ScheduleForm
      mode="create"
      title="New schedule"
      headerDescription="You’re reminded when it’s due; nothing posts automatically."
      breadcrumbs={[{ label: PAGE_META.scheduled.title, href: resolve('/scheduled') }]}
      onSaved={handleSaved}
      onCancel={() => void goto(resolve('/scheduled'))}
    />
  </div>
</BookGate>
