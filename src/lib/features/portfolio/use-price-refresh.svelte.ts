import { toast } from '$lib/components/ui/sonner';
import { toastPriceResult } from '$lib/features/portfolio/price-utils';
import { dbSession } from '$lib/stores/db.svelte';
import { refreshPrices } from '$lib/tauri';
import { errMsg } from '$lib/utils/error';

interface PriceFetchError {
  mnemonic: string;
  error: string;
}

/**
 * Shared Yahoo price-refresh state for portfolio pages. Owns the
 * `refreshing` flag + per-commodity `errors` so a fix in one page lands in
 * the other. Pass a `reload` that refetches page data after new prices land.
 */
export function createPriceRefresh(reload: () => void) {
  let refreshing = $state(false);
  let errors = $state<PriceFetchError[]>([]);

  async function refresh() {
    if (!dbSession.value || refreshing) return;
    refreshing = true;
    errors = [];
    try {
      const result = await refreshPrices();
      errors = result.errors;
      toastPriceResult(result);
      // Reload so market values, day change and timestamps pick up the new prices.
      reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t refresh prices'));
    } finally {
      refreshing = false;
    }
  }

  return {
    get refreshing() {
      return refreshing;
    },
    get errors() {
      return errors;
    },
    refresh
  };
}
