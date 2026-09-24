<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { toast } from '$lib/components/ui/sonner';
  import ScheduleForm from '$lib/features/schedules/schedule-form.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import type { Schedule } from '$lib/tauri';

  function handleSaved(schedule: Schedule) {
    toast.success(`“${schedule.name}” is on repeat now`);
    void goto(resolve('/scheduled'), { replaceState: true });
  }
</script>

<svelte:head>
  <title>New schedule · Spendo</title>
  <meta name="description" content="Create a recurring schedule with template splits." />
</svelte:head>

<BookGate title="New schedule" description="Set it once. We’ll remind you each time it’s due">
  <div class="mx-auto flex w-full max-w-5xl flex-col gap-6">
    <ScheduleForm
      mode="create"
      title="New schedule"
      headerDescription="Set it once. We’ll remind you each time it’s due. Nothing posts without you."
      onSaved={handleSaved}
      onCancel={() => void goto(resolve('/scheduled'))}
    />
  </div>
</BookGate>
