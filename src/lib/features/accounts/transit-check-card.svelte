<script lang="ts">
  import { resolve } from '$app/paths';
  import { ArrowLeftRight } from '@lucide/svelte';
  import * as Card from '$lib/components/ui/card';
  import * as Table from '$lib/components/ui/table';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import Amount from '$lib/components/shared/amount.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import { fetchTransitCheck } from '$lib/tauri';
  import { dbSession } from '$lib/stores/db.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import { openTransaction } from '$lib/utils/navigation';

  let {
    version = null
  }: {
    /** Any value that changes when the account’s ledger reloads; triggers a refetch. */
    version?: unknown;
  } = $props();

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      void version;
      return fetchTransitCheck();
    },
    label: 'We couldn’t check this account’s transfers'
  });

  let legs = $derived(res.data?.openLegs ?? []);
</script>

<Card.Root>
  <IconCardHeader
    title="Unmatched transfers"
    description="Legs with no opposite amount within 14 days, oldest first"
  >
    {#snippet icon()}
      <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content>
    {#if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if res.loading && !res.data}
      <Skeleton class="h-16 w-full" />
    {:else if legs.length === 0}
      <p class="text-sm text-muted-foreground">
        Every transfer here has both sides. Nice and tidy.
      </p>
    {:else}
      <p class="mb-3 text-sm text-muted-foreground">
        {plural(legs.length, 'open leg')} ·
        <Amount value={res.data?.openTotal ?? 0} /> waiting for the other side
      </p>
      <Table.Root>
        <Table.Header>
          <Table.Row>
            <Table.Head>Date</Table.Head>
            <Table.Head>Description</Table.Head>
            <Table.Head>Other side</Table.Head>
            <Table.Head class="text-right">Open for</Table.Head>
            <Table.Head class="text-right">Amount</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each legs as leg (leg.txGuid)}
            <NavRow onNavigate={() => void openTransaction(leg.txGuid)}>
              <Table.Cell class="whitespace-nowrap tabular-nums">
                {formatDate(leg.postDate)}
              </Table.Cell>
              <Table.Cell>
                <a
                  href={resolve('/transactions/[guid]', { guid: leg.txGuid })}
                  class="hover:underline"
                >
                  {leg.description || 'No description'}
                </a>
              </Table.Cell>
              <Table.Cell>
                {#if leg.counterAccount}{leg.counterAccount}{:else}<EmptyValue />{/if}
              </Table.Cell>
              <Table.Cell class="text-right whitespace-nowrap tabular-nums">
                {plural(leg.daysOpen, 'day')}
              </Table.Cell>
              <Table.Cell class="text-right"><Amount value={leg.amount} /></Table.Cell>
            </NavRow>
          {/each}
        </Table.Body>
      </Table.Root>
    {/if}
  </Card.Content>
</Card.Root>
