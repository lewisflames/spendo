<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import { ArrowLeft, CheckCircle2 } from '@lucide/svelte';
  import type { CsvCommitResult } from '$lib/tauri';
  import { plural } from '$lib/utils/format.svelte';

  let {
    commitResult,
    onReset
  }: {
    commitResult: CsvCommitResult;
    onReset: () => void;
  } = $props();
</script>

<Card.Root>
  <Card.Header>
    <Card.Title class="flex items-center gap-2">
      <CheckCircle2 aria-hidden="true" class="size-5 text-green-600" />
      Import complete
    </Card.Title>
    <Card.Description>{commitResult.label}</Card.Description>
  </Card.Header>
  <Card.Content class="flex flex-col gap-4">
    <p class="text-sm text-muted-foreground">
      Created {plural(commitResult.created, 'transaction')} · skipped {plural(
        commitResult.skippedDuplicates,
        'duplicate'
      )} · skipped {plural(commitResult.skippedErrors, 'error')}. Revert from Settings → History if
      needed.
    </p>
    <div class="flex flex-wrap gap-2">
      <Button href={resolve('/settings')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Back to Settings
      </Button>
      <Button variant="outline" onclick={onReset}>Import another file</Button>
    </div>
  </Card.Content>
</Card.Root>
