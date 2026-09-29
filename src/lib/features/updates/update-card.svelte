<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { ExternalLink, LoaderCircle, RefreshCw } from '@lucide/svelte';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Progress } from '$lib/components/ui/progress';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import {
    checkForUpdates,
    installUpdate,
    selfUpdateSupported,
    updates
  } from '$lib/stores/updates.svelte';

  const AUR_URL = 'https://aur.archlinux.org/packages/spendo-bin';
  const AUR_COMMAND = 'yay -Syu spendo-bin';

  let percent = $derived(
    updates.value.progress === null ? null : Math.round(updates.value.progress * 100)
  );

  function handleCheck() {
    void checkForUpdates();
  }

  function handleInstall() {
    void installUpdate();
  }

  function handleRetry() {
    if (selfUpdateSupported && updates.value.version) {
      void installUpdate();
    } else {
      void checkForUpdates();
    }
  }

  function handleViewAur() {
    // Native opener; a failure is non-fatal, the command is on screen.
    void openUrl(AUR_URL).catch(() => {});
  }
</script>

<Card.Root>
  <IconCardHeader title="Updates">
    {#snippet icon()}
      <RefreshCw aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col items-start gap-4">
    <div aria-live="polite" class="flex w-full flex-col items-start gap-3">
      {#if updates.value.status === 'idle'}
        <p class="text-sm text-muted-foreground">Check for a newer release of Spendo</p>
        <Button onclick={handleCheck}>Check for updates</Button>
      {:else if updates.value.status === 'checking'}
        <Button disabled>
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Checking…
        </Button>
      {:else if updates.value.status === 'up-to-date'}
        <p class="text-sm text-muted-foreground">You’re on the latest version.</p>
        <Button variant="outline" onclick={handleCheck}>Check again</Button>
      {:else if updates.value.status === 'available'}
        <p class="text-sm">
          Version <span class="font-medium">{updates.value.version}</span> is available.
        </p>
        {#if updates.value.notes}
          <p class="line-clamp-4 text-sm whitespace-pre-line text-muted-foreground">
            {updates.value.notes}
          </p>
        {/if}
        {#if selfUpdateSupported}
          <Button onclick={handleInstall}>
            <RefreshCw aria-hidden="true" class="size-4" />
            Download &amp; install
          </Button>
        {:else}
          <p class="text-sm text-muted-foreground">
            Spendo was installed from the AUR, so update it through your package manager rather than
            here.
          </p>
          <code class="block rounded-md bg-muted px-2 py-1 font-mono text-xs">{AUR_COMMAND}</code>
          <div class="flex flex-wrap items-center gap-2">
            <Button variant="outline" onclick={handleViewAur}>
              <ExternalLink aria-hidden="true" class="size-4" />
              View on AUR
            </Button>
            <Button variant="outline" onclick={handleCheck}>Check again</Button>
          </div>
        {/if}
      {:else if updates.value.status === 'downloading'}
        <p class="text-sm">
          Downloading update…{percent === null ? '' : ` ${percent}%`}
        </p>
        <Progress value={percent ?? null} class="w-full" />
      {:else}
        <p class="text-sm text-destructive">{updates.value.error}</p>
        <Button variant="outline" onclick={handleRetry}>Try again</Button>
      {/if}
    </div>
  </Card.Content>
</Card.Root>
