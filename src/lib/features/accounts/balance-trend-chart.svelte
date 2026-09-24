<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchAccountBalanceTrend } from '$lib/tauri';
  import type { AccountBalancePoint } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import { TYPE_COLOR } from '$lib/features/accounts/account-types';

  // `accountType` arrives with the detail payload while the trend resolves on
  // its own fetch. Unknown reads as `undefined` (never omit the prop), so the
  // first paint waits for the final color instead of drawing with the
  // fallback and flashing to the type color once detail arrives.
  let { guid, accountType }: { guid: string; accountType: string | undefined } = $props();

  let color = $derived(TYPE_COLOR[accountType ?? ''] ?? 'var(--color-primary)');

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!guid,
    // Reads `guid`, so navigating between accounts refetches automatically.
    fetch: () => fetchAccountBalanceTrend(guid),
    label: 'We couldn’t load the balance chart'
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
    description="How the balance moved, transaction by transaction"
    {loading}
    emptyMessage="Not enough history to draw a line yet."
    {data}
    getX={(d: AccountBalancePoint) => parseLocalDate(d.date)}
    series={[{ key: 'balance', label: 'Balance', color }]}
    {currency}
  />
{/if}
