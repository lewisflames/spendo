<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { deletePayee, fetchPayees, renamePayee, type PayeeSummary } from '$lib/tauri';
  import {
    MAX_PAYEE_LEN,
    normalizePayeeName,
    payeeListHref,
    payeeNameError
  } from '$lib/features/payees/payee-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { errMsg } from '$lib/utils/error';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Check, Pencil, Store, Trash2, X } from '@lucide/svelte';

  const payeesRes = useAsyncData({
    enabled: () => true,
    fetch: fetchPayees,
    label: 'We couldn’t load your payees'
  });
  const payees = $derived(payeesRes.data ?? []);

  /** Lowercase without accents, so “victor” finds both Victor and Víctor. */
  function fold(text: string): string {
    return text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
  }

  let query = $state('');
  let shown = $derived.by(() => {
    const q = fold(query.trim());
    return q ? payees.filter((p) => fold(p.name).includes(q)) : payees;
  });

  let editingGuid = $state<string | null>(null);
  let draft = $state('');
  let busyGuid = $state<string | null>(null);
  let confirmDeleteGuid = $state<string | null>(null);

  let draftError = $derived.by(() => {
    if (!editingGuid) return null;
    const name = normalizePayeeName(draft);
    return name ? payeeNameError(name) : 'Payees need a name';
  });
  // Renaming onto another payee's name merges the two; say so before saving.
  let mergeTarget = $derived.by(() => {
    if (!editingGuid) return null;
    const lower = normalizePayeeName(draft).toLowerCase();
    return payees.find((p) => p.guid !== editingGuid && p.name.toLowerCase() === lower) ?? null;
  });

  function startEdit(payee: PayeeSummary) {
    editingGuid = payee.guid;
    draft = payee.name;
    confirmDeleteGuid = null;
  }

  async function saveEdit(payee: PayeeSummary) {
    if (busyGuid || draftError) return;
    const name = normalizePayeeName(draft);
    if (name === payee.name) {
      editingGuid = null;
      return;
    }
    busyGuid = payee.guid;
    try {
      await renamePayee(payee.guid, name);
      toast.success(mergeTarget ? `Merged into ${mergeTarget.name}` : 'Payee renamed');
      editingGuid = null;
      payeesRes.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t rename this payee'));
    } finally {
      busyGuid = null;
    }
  }

  async function handleDelete(payee: PayeeSummary) {
    if (busyGuid) return;
    if (confirmDeleteGuid !== payee.guid) {
      confirmDeleteGuid = payee.guid;
      return;
    }
    busyGuid = payee.guid;
    try {
      await deletePayee(payee.guid);
      confirmDeleteGuid = null;
      payeesRes.reload();
      toast.success('Payee deleted');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t delete this payee'));
    } finally {
      busyGuid = null;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Payees"
    description="Who your transactions were with, added from the transaction form or bulk edit. Rename one onto another’s name to merge them."
  >
    {#snippet icon()}
      <Store aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3">
    {#if payeesRes.loading && !payeesRes.data}
      <p class="py-2 text-sm text-muted-foreground">Finding your payees…</p>
    {:else if payeesRes.error}
      <ErrorCard message={payeesRes.error} onRetry={() => payeesRes.reload()} />
    {:else if payees.length === 0}
      <p class="text-sm text-muted-foreground">
        No payees yet. Add one to any transaction: the shop, the person or the employer.
      </p>
    {:else}
      <Input
        type="search"
        bind:value={query}
        aria-label="Find a payee"
        placeholder="Find a payee…"
        autocomplete="off"
      />
      <p class="text-xs text-muted-foreground">
        {query.trim()
          ? `${shown.length} of ${plural(payees.length, 'payee')}`
          : plural(payees.length, 'payee')}
      </p>
      <ul class="flex max-h-96 flex-col gap-1.5 overflow-y-auto">
        {#each shown as payee (payee.guid)}
          <li class="flex items-center gap-2 rounded-lg border px-3 py-2">
            {#if editingGuid === payee.guid}
              <form
                class="flex min-w-0 flex-1 flex-col gap-1"
                onsubmit={(e) => {
                  e.preventDefault();
                  void saveEdit(payee);
                }}
              >
                <div class="flex items-center gap-2">
                  <Input
                    bind:value={draft}
                    aria-label="Payee name"
                    maxlength={MAX_PAYEE_LEN + 10}
                    aria-invalid={draftError ? true : undefined}
                    class="h-7"
                    autofocus
                    onkeydown={(e) => {
                      if (e.key === 'Escape') editingGuid = null;
                    }}
                  />
                  <Button
                    type="submit"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Save payee name"
                    disabled={!!draftError || busyGuid !== null}
                  >
                    <Check aria-hidden="true" class="size-4" />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Cancel renaming"
                    onclick={() => (editingGuid = null)}
                  >
                    <X aria-hidden="true" class="size-4" />
                  </Button>
                </div>
                {#if draftError}
                  <p role="alert" class="text-xs text-destructive">{draftError}</p>
                {:else if mergeTarget}
                  <p class="text-xs text-muted-foreground">
                    Saving merges this payee into {mergeTarget.name}.
                  </p>
                {/if}
              </form>
            {:else}
              <div class="flex min-w-0 flex-1 flex-col">
                <!-- eslint-disable svelte/no-navigation-without-resolve -- payeeListHref resolves the path -->
                <a
                  href={payeeListHref(payee.guid)}
                  class="truncate text-sm font-medium hover:underline focus-visible:outline-2 focus-visible:outline-ring"
                  {@attach tooltip(payee.name, { truncated: true })}
                >
                  {payee.name}
                </a>
                <!-- eslint-enable svelte/no-navigation-without-resolve -->
                <span class="text-xs text-muted-foreground">
                  {plural(payee.transactionCount, 'transaction')}{payee.lastUsed
                    ? ` · last on ${formatDate(payee.lastUsed)}`
                    : ''}
                </span>
              </div>
              <div class="flex shrink-0 items-center gap-0.5">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Rename payee {payee.name}"
                  {@attach tooltip('Rename')}
                  disabled={busyGuid !== null}
                  onclick={() => startEdit(payee)}
                >
                  <Pencil aria-hidden="true" class="size-4" />
                </Button>
                {#if confirmDeleteGuid === payee.guid}
                  <Button
                    variant="destructive"
                    size="sm"
                    onclick={() => void handleDelete(payee)}
                    disabled={busyGuid !== null}
                  >
                    {busyGuid === payee.guid ? 'Deleting…' : 'Confirm delete'}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onclick={() => (confirmDeleteGuid = null)}
                    disabled={busyGuid !== null}
                  >
                    Cancel
                  </Button>
                {:else}
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    class="text-destructive hover:text-destructive"
                    aria-label="Delete payee {payee.name}"
                    {@attach tooltip('Delete. Transactions stay, just without a payee')}
                    disabled={busyGuid !== null}
                    onclick={() => void handleDelete(payee)}
                  >
                    <Trash2 aria-hidden="true" class="size-4" />
                  </Button>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </Card.Content>
</Card.Root>
