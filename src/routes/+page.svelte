<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import WelcomeScreen from '$lib/features/welcome/welcome-screen.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';

  // `/` is welcome-only: an open book lands on `/transactions` instead.
  // Gated on bootstrap (not refresh loading) so the layout’s book-less guard
  // and this redirect can’t fight during cold start.
  $effect(() => {
    if (!dbBootstrapping.value && dbSession.value) {
      void goto(resolve('/transactions'), { replaceState: true });
    }
  });
</script>

<svelte:head>
  <title>Welcome · Spendo</title>
  <meta
    name="description"
    content="Spendo explores your finances locally. Open or create a book to get started."
  />
</svelte:head>

{#if dbBootstrapping.value && !dbSession.value}
  <PageSkeleton variant="welcome" />
{:else if !dbSession.value}
  <WelcomeScreen />
{/if}
