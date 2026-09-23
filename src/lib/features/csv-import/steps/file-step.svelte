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
    <Card.Title>Bring in a bank file</Card.Title>
    <Card.Description>
      Bank exports (.csv, .txt, .tsv). Nothing lands in your book until the last step.
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
      We’ll peek at the encoding and columns first. Nothing is saved yet.
    </p>
  </Card.Content>
</Card.Root>
