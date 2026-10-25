<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import { Select as SelectPrimitive } from 'bits-ui';
  import Amount from '$lib/components/shared/amount.svelte';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { ReceiptText } from '@lucide/svelte';
  import { saveBookSetting } from '$lib/stores/book-settings.svelte';
  import { errMsg } from '$lib/utils/error';
  import { plural } from '$lib/utils/format.svelte';
  import type { TagSummary, TaxSummary } from '$lib/tauri';
  import { yearRange } from './tax-utils';

  let {
    summary,
    tags,
    onChanged
  }: {
    summary: TaxSummary;
    /** Every tag in the book, most used first. */
    tags: TagSummary[];
    onChanged: () => void;
  } = $props();

  /** Select value for "no deductible tag" (bits-ui items need a non-empty value). */
  const NONE = '__none';

  let saving = $state(false);
  let d = $derived(summary.deductible);
  let items = $derived([
    { value: NONE, label: 'None' },
    ...tags.map((t) => ({ value: t.guid, label: t.name }))
  ]);
  let listHref = $derived.by(() => {
    if (!d) return null;
    const { from, to } = yearRange(summary.year);
    return `${resolve('/transactions')}?tag=${encodeURIComponent(d.tagGuid)}&from=${from}&to=${to}`;
  });

  async function pick(value: string) {
    if (saving) return;
    saving = true;
    try {
      await saveBookSetting('taxDeductibleTag', value === NONE ? '' : value);
      onChanged();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t save that'));
    } finally {
      saving = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Deductible expenses"
    description="Tag expenses you can deduct, like donations or pension contributions"
  >
    {#snippet icon()}
      <ReceiptText aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3 text-sm">
    <div class="flex items-center justify-between gap-3">
      <label for="tax-deductible-tag" class="text-muted-foreground">Tag</label>
      <Select.Root
        type="single"
        value={d?.tagGuid ?? NONE}
        onValueChange={(v) => void pick(v)}
        {items}
        disabled={saving || tags.length === 0}
      >
        <Select.Trigger id="tax-deductible-tag" class="w-44">
          <SelectPrimitive.Value placeholder="None" />
        </Select.Trigger>
        <Select.Content>
          {#each items as item (item.value)}
            <Select.Item value={item.value} label={item.label}>{item.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    {#if d}
      <div class="flex items-baseline justify-between gap-3">
        <span class="font-medium">{plural(d.transactions, 'transaction')} in {summary.year}</span>
        <Amount value={d.total} weight="semibold" />
      </div>
      {#if listHref}
        <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
        <a href={listHref} class="text-xs font-medium underline-offset-2 hover:underline">
          See them in Transactions
        </a>
      {/if}
    {:else if tags.length === 0}
      <p class="text-xs text-muted-foreground">
        Add a tag to a transaction first, then pick it here.
      </p>
    {/if}
  </Card.Content>
</Card.Root>
