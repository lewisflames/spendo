<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import { ArrowRight, CheckCircle2 } from '@lucide/svelte';
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
      )} · skipped {plural(commitResult.skippedErrors, 'error')}.
    </p>
    <div class="flex flex-wrap gap-2">
      <Button href={resolve('/transactions')}>
        <ArrowRight aria-hidden="true" class="size-4" />
        View transactions
      </Button>
      <Button variant="outline" onclick={onReset}>Import another file</Button>
    </div>
    <a
      class="text-sm text-muted-foreground underline underline-offset-4"
      href={resolve('/settings')}>Back to settings</a
    >
  </Card.Content>
</Card.Root>
