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

  // `accountType` arrives with the detail payload while the trend resolves on
  // its own fetch. Unknown reads as `undefined` (never omit the prop), so the
  // first paint waits for the final color instead of drawing with the
  // fallback and flashing to the type color once detail arrives.
  let {
    guid,
    accountType,
    asOf
  }: { guid: string; accountType: string | undefined; asOf?: string } = $props();

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
  // Hold the skeleton until the account type is known: painting with the
  // fallback color first and recoloring on detail arrival is a visible flash.
  let loading = $derived(res.loading || accountType == null);
  let currency = $derived(sessionCurrency());
</script>

{#if res.error}
  <ErrorCard message={res.error} onRetry={() => res.reload()} />
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
