<script lang="ts">
  let {
    name,
    description,
    code = null
  }: {
    name: string;
    description?: string | null;
    /** Optional account code, shown as a small chip after the name. */
    code?: string | null;
  } = $props();
</script>

<span class="flex min-w-0 flex-col">
  <span class="flex min-w-0 items-center gap-1.5">
    <span class="min-w-0 truncate font-medium" title={name}>{name}</span>
    {#if code}
      <span
        class="shrink-0 rounded bg-muted px-1 py-0.5 text-[10px] leading-none font-normal text-muted-foreground tabular-nums"
        title={`Code ${code}`}>{code}</span
      >
    {/if}
  </span>
  {#if description && description !== name}
    <span class="min-w-0 truncate text-xs text-muted-foreground" title={description}>
      {description}
    </span>
  {/if}
</span>
