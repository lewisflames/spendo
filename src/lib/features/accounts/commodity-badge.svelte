<script lang="ts">
  let {
    commodity,
    reportCurrency,
    title
  }: {
    commodity: string | null;
    reportCurrency: string;
    title?: string;
  } = $props();
</script>

{#if commodity && commodity !== reportCurrency}
  <span
    class="ml-1 rounded-sm border border-border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
    title={title ?? (commodity ? `Balance converted from ${commodity}` : undefined)}
  >
    {commodity}
  </span>
{/if}
