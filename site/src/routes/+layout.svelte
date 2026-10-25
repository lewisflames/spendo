<script lang="ts">
  import '../app.css';
  import icon from '$icon';

  let { children } = $props();
</script>

<svelte:head>
  <link rel="icon" type="image/svg+xml" href={icon} />
</svelte:head>

{@render children()}
