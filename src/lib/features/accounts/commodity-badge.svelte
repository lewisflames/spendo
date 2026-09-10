<script lang="ts">
  let {
    commodity,
    reportCurrency
  }: {
    commodity: string | null;
    reportCurrency: string;
  } = $props();
</script>

{#if commodity && commodity !== reportCurrency}
  <span
    class="ml-1 rounded-sm border border-border px-1 text-[10px] leading-4 font-medium text-muted-foreground"
    title={`Balance converted from ${commodity}`}
  >
    {commodity}
  </span>
{/if}
