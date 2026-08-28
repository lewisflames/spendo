<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccountBalanceTrend } from '$lib/tauri';
  import type { AccountBalancePoint } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { sessionCurrency } from '$lib/utils/format.svelte';
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';

  let { guid, accountType }: { guid: string; accountType?: string } = $props();

  let color = $derived(TYPE_COLOR[accountType ?? ''] ?? 'var(--color-primary)');

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    // Reads `guid`, so navigating between accounts refetches automatically.
    fetch: () => fetchAccountBalanceTrend(guid),
    label: 'Failed to load balance trend'
  });

  let data = $derived(res.data ?? []);
  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let currency = $derived(sessionCurrency());
</script>

{#if error}
  <ErrorCard message={error} onRetry={() => res.reload()} />
{:else}
  <TimeSeriesChart
    title="Balance Over Time"
    description="Account balance after each transaction"
    {loading}
    emptyMessage="No balance history found."
    {data}
    getX={(d: AccountBalancePoint) => new Date(d.month)}
    series={[{ key: 'balance', label: 'Balance', color }]}
    {currency}
  />
{/if}
