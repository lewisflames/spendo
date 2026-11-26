<script lang="ts">
  import { resolve } from '$app/paths';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { ArrowLeft, CircleCheck, EyeOff, FileWarning, FolderOpen } from '@lucide/svelte';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/components/ui/sonner';
  import Amount from '$lib/components/shared/amount.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import OverflowMenu from '$lib/components/shared/overflow-menu.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { openBookAt } from '$lib/features/book/book-actions';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { dismissSyncConflict } from '$lib/stores/sync-conflicts.svelte';
  import { reviewSyncConflict } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import ConflictTxTable from './conflict-tx-table.svelte';
  import { conflictFieldLabel } from './sync-conflict-utils';

  const meta = PAGE_META.syncConflict;

  let fileName = $derived(page.url.searchParams.get('file') ?? '');

  const res = useAsyncData({
    enabled: () => !!dbSession.value && fileName !== '',
    fetch: () => reviewSyncConflict(fileName),
    label: 'We couldn’t compare that copy'
  });

  let review = $derived(res.data);
  let differences = $derived(
    review ? review.onlyInCopy.length + review.onlyInBook.length + review.changed.length : 0
  );
  let savedAt = $derived(
    review?.copy.savedAt
      ? formatDate(review.copy.savedAt, { dateStyle: 'medium', timeStyle: 'short' })
      : null
  );

  function dismiss() {
    if (!review) return;
    dismissSyncConflict(review.copy.path);
    toast.success('Conflicting copy dismissed. The file stays in its folder.');
    void goto(resolve('/'));
  }

  async function reveal() {
    if (!review) return;
    try {
      await revealItemInDir(review.copy.path);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t open that folder'));
    }
  }
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    {#if (res.loading || dbBootstrapping.value) && !review}
      <PageSkeleton variant="transaction-detail" />
    {:else if review}
      {@const r = review}
      <PageHeader
        title={savedAt ? `${meta.title} from ${savedAt}` : meta.title}
        breadcrumbs={[{ label: PAGE_META.overview.title, href: resolve('/') }]}
      >
        {#snippet description()}
          {r.copy.fileName}
        {/snippet}
        <Button onclick={() => void openBookAt(r.copy.path)}>Open that copy</Button>
        <OverflowMenu
          actions={[
            { label: 'Show in folder', icon: FolderOpen, onSelect: () => void reveal() },
            { label: 'Dismiss', icon: EyeOff, onSelect: dismiss }
          ]}
        />
      </PageHeader>

      <p class="text-sm text-muted-foreground">
        Spendo only reads this copy: nothing is merged, changed or deleted. Re-enter anything you
        want to keep in this book, then delete the copy from its folder. Only transactions are
        compared, matched by their ID{r.copy.device ? ` (copy from device ${r.copy.device})` : ''}
      </p>

      <dl class="flex flex-wrap gap-x-8 gap-y-3 rounded-lg border bg-muted/25 px-4 py-3 text-sm">
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Only in the copy</dt>
          <dd class="font-medium tabular-nums">{r.onlyInCopy.length}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Only in this book</dt>
          <dd class="font-medium tabular-nums">{r.onlyInBook.length}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Changed</dt>
          <dd class="font-medium tabular-nums">{r.changed.length}</dd>
        </div>
        <div class="flex flex-col gap-0.5">
          <dt class="text-xs text-muted-foreground">Identical</dt>
          <dd class="font-medium tabular-nums">{r.unchanged}</dd>
        </div>
      </dl>

      {#if differences === 0}
        <EmptyState
          card
          title="Nothing differs"
          description="Every transaction matches this book. You can delete the copy from its folder."
        >
          {#snippet icon()}
            <CircleCheck aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
        </EmptyState>
      {/if}

      {#if r.onlyInCopy.length > 0}
        <section class="flex flex-col gap-2" aria-labelledby="only-in-copy">
          <h2 id="only-in-copy" class="font-medium">
            Only in the copy
            <span class="text-sm font-normal text-muted-foreground">
              {plural(r.onlyInCopy.length, 'transaction')} missing here
            </span>
          </h2>
          <ConflictTxTable rows={r.onlyInCopy} linked={false} />
        </section>
      {/if}

      {#if r.onlyInBook.length > 0}
        <section class="flex flex-col gap-2" aria-labelledby="only-in-book">
          <h2 id="only-in-book" class="font-medium">
            Only in this book
            <span class="text-sm font-normal text-muted-foreground">
              added here, or deleted in the copy
            </span>
          </h2>
          <ConflictTxTable rows={r.onlyInBook} linked />
        </section>
      {/if}

      {#if r.changed.length > 0}
        <section class="flex flex-col gap-2" aria-labelledby="changed">
          <h2 id="changed" class="font-medium">Changed</h2>
          <TableShell>
            <Table.Header>
              <Table.Row class="bg-muted/75">
                <Table.Head class="text-muted-foreground">Date</Table.Head>
                <Table.Head class="text-muted-foreground">Description</Table.Head>
                <Table.Head class="text-muted-foreground">What differs</Table.Head>
                <Table.Head class="text-right text-muted-foreground">This book</Table.Head>
                <Table.Head class="text-right text-muted-foreground">Copy</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each r.changed as change (change.book.guid)}
                {@const b = change.book}
                {@const c = change.copy}
                <Table.Row>
                  <Table.Cell class="py-2.5 whitespace-nowrap tabular-nums">
                    {formatDate(b.postDate)}
                    {#if c.postDate !== b.postDate}
                      <p class="text-xs text-muted-foreground">Copy: {formatDate(c.postDate)}</p>
                    {/if}
                  </Table.Cell>
                  <Table.Cell class="max-w-xs truncate py-2.5 font-medium">
                    <a
                      class="hover:underline"
                      href={resolve('/transactions/[guid]', { guid: b.guid })}
                    >
                      {b.description || 'Transaction'}
                    </a>
                    {#if (c.description ?? '') !== (b.description ?? '')}
                      <p class="truncate text-xs font-normal text-muted-foreground">
                        Copy: {c.description || 'no description'}
                      </p>
                    {/if}
                  </Table.Cell>
                  <Table.Cell class="py-2.5">
                    <div class="flex flex-wrap gap-1">
                      {#each change.fields as field (field)}
                        <Badge variant="secondary">{conflictFieldLabel(field)}</Badge>
                      {/each}
                    </div>
                  </Table.Cell>
                  <Table.Cell class="py-2.5 text-right"><Amount value={b.amount} /></Table.Cell>
                  <Table.Cell class="py-2.5 text-right"><Amount value={c.amount} /></Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </TableShell>
        </section>
      {/if}
    {:else if res.errorKind === 'notFound' || fileName === ''}
      <PageHeader
        title={meta.title}
        breadcrumbs={[{ label: PAGE_META.overview.title, href: resolve('/') }]}
      />
      <EmptyState
        card
        title="That copy is gone"
        description="It may have been deleted or renamed in its folder."
      >
        {#snippet icon()}
          <FileWarning aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
        {#snippet action()}
          <Button variant="outline" href={resolve('/')}>
            <ArrowLeft aria-hidden="true" class="size-4" />
            Overview
          </Button>
        {/snippet}
      </EmptyState>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
