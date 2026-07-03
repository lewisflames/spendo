export function useQuery<T>(
  fetcher: () => Promise<T>,
  guard: () => boolean
): { data: T | null; loading: boolean; error: string | null; refetch: () => void } {
  let data = $state<T | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let key = $state(0);

  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    key;
    if (!guard()) {
      data = null;
      loading = false;
      error = null;
      return;
    }
    loading = true;
    error = null;
    let cancelled = false;
    fetcher()
      .then((r) => {
        if (cancelled) return;
        data = r;
        loading = false;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error = e instanceof Error ? e.message : 'An error occurred';
        loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  function refetch() {
    key++;
  }

  return {
    get data() {
      return data;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    refetch
  };
}
