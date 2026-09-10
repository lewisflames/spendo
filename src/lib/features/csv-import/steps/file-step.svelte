<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { FileUp, LoaderCircle } from '@lucide/svelte';

  let {
    detectError,
    picking,
    onDetect
  }: {
    detectError: string | null;
    picking: boolean;
    onDetect: () => void;
  } = $props();
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Choose a CSV file</Card.Title>
    <Card.Description>
      Bank-statement exports (.csv, .txt, .tsv). Nothing is written until the final import step.
    </Card.Description>
  </Card.Header>
  <Card.Content class="flex flex-col gap-4">
    {#if detectError}
      <ErrorCard message={detectError} onRetry={onDetect} />
    {/if}
    <div>
      <Button onclick={onDetect} disabled={picking}>
        {#if picking}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Reading…
        {:else}
          <FileUp aria-hidden="true" class="size-4" />
          Choose CSV file
        {/if}
      </Button>
    </div>
    <p class="text-xs text-muted-foreground">
      Detection runs locally: encoding, delimiter and header row are sniffed before mapping.
    </p>
  </Card.Content>
</Card.Root>
