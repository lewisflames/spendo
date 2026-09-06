<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccountBalanceTrend } from '$lib/tauri';
  import type { AccountBalancePoint } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import { todayStr } from '$lib/utils/date-utils.svelte';
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';

  let { guid, accountType, asOf }: { guid: string; accountType?: string; asOf?: string } = $props();

  let color = $derived(TYPE_COLOR[accountType ?? ''] ?? 'var(--color-primary)');

  // Marker for the as-of cutoff within the full history — shown only for
  // past dates (today needs no marker, future is capped at today).
  let referenceLine = $derived.by(() => {
    if (!asOf || !/^\d{4}-\d{2}-\d{2}$/.test(asOf) || asOf >= todayStr()) return null;
    const date = parseLocalDate(asOf);
    if (Number.isNaN(date.getTime())) return null;
    return { date };
  });

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    // Reads `guid`, so navigating between accounts refetches automatically.
    // The trend always shows full history; `asOf` only drives the marker.
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
    getX={(d: AccountBalancePoint) => parseLocalDate(d.date)}
    series={[{ key: 'balance', label: 'Balance', color }]}
    {currency}
    {referenceLine}
  />
{/if}
