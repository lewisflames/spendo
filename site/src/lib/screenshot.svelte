<script lang="ts">
  import type { Screenshot } from './screenshots';

  let {
    shot,
    eager = false,
    class: className = ''
  }: { shot: Screenshot; eager?: boolean; class?: string } = $props();
</script>

<!-- Follows the visitor's colour scheme, like the rest of the page. -->
<picture>
  <source srcset={shot.dark} media="(prefers-color-scheme: dark)" />
  <img
    src={shot.light}
    alt={shot.alt}
    width="2560"
    height="1760"
    loading={eager ? 'eager' : 'lazy'}
    decoding="async"
    class="h-auto w-full rounded-xl border shadow-lg {className}"
  />
</picture>
