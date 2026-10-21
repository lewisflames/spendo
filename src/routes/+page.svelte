<script lang="ts">
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import WelcomeScreen from '$lib/features/welcome/welcome-screen.svelte';
  import Overview from '$lib/features/overview/overview.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';

  // `/` is home: the Overview with a book open, the welcome screen without one.
</script>

<svelte:head>
  <title>{dbSession.value ? PAGE_META.overview.title : 'Welcome'} · Spendo</title>
  <meta
    name="description"
    content={dbSession.value
      ? PAGE_META.overview.description
      : 'Spendo keeps your finances on this machine. Open or create a book to get started.'}
  />
</svelte:head>

{#if dbSession.value}
  <Overview />
{:else if dbBootstrapping.value}
  <PageSkeleton variant="welcome" />
{:else}
  <WelcomeScreen />
{/if}
