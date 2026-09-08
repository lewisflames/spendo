<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import * as Table from '$lib/components/ui/table';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { toast } from '$lib/components/ui/sonner';
  import { open } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { resolve } from '$app/paths';
  import { SvelteSet } from 'svelte/reactivity';
  import {
    ArrowLeft,
    ArrowRight,
    CheckCircle2,
    FileUp,
    LoaderCircle,
    Upload
  } from '@lucide/svelte';
  import {
    csvCommit,
    csvDetect,
    csvPreview,
    fetchAccounts,
    fetchHomeData,
    type CsvCommitResult,
    type CsvDetectResult,
    type CsvPreviewResult
  } from '$lib/tauri';
  import { dbSession, setDbSession } from '$lib/stores/db.svelte';
  import { errMsg } from '$lib/utils/error';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { guessMapping } from '$lib/features/csv-import/guess';
  import { formatCurrency, formatDate, plural, sessionCurrency } from '$lib/utils/format.svelte';

  type Step = 'file' | 'mapping' | 'preview' | 'done';

  let step = $state<Step>('file');
  let path = $state('');
  let filename = $state('');
  let detect = $state<CsvDetectResult | null>(null);

  let delimiter = $state(',');
  let hasHeader = $state(true);
  let datePreference = $state<'dmy' | 'mdy'>('dmy');

  let usePair = $state(false);
  let dateCol = $state(0);
  let descCol = $state(1);
  let amountCol = $state<number | null>(2);
  let debitCol = $state<number | null>(null);
  let creditCol = $state<number | null>(null);

  let targetAccountGuid = $state('');
  let offsetAccountGuid = $state('');

  let preview = $state<CsvPreviewResult | null>(null);
  let selected = new SvelteSet<number>();
  let page = $state(1);
  let pageSize = $state(50);

  let picking = $state(false);
  let previewing = $state(false);
  let committing = $state(false);
  let detectError = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let commitError = $state<string | null>(null);
  let commitResult = $state<CsvCommitResult | null>(null);

  let accountsData = $state<{ trees: Parameters<typeof flattenAccountsForPicker>[0] } | null>(null);
  let pickerOptions = $derived(
    flattenAccountsForPicker(accountsData?.trees, { hidePlaceholders: true })
  );

  // Load account trees once a book is present (for both pickers).
  $effect(() => {
    if (!dbSession.value) return;
    let cancelled = false;
    fetchAccounts()
      .then((r) => {
        if (!cancelled) accountsData = r;
      })
      .catch(() => {
        if (!cancelled) accountsData = null;
      });
    return () => {
      cancelled = true;
    };
  });

  let currency = $derived(sessionCurrency());

  let columnOptions = $derived.by(() => {
    const headers = detect?.headers ?? [];
    const count = detect?.columnCount ?? 0;
    return Array.from({ length: count }, (_, i) => ({
      value: String(i),
      label: `${i + 1}. ${headers[i] || `Column ${i + 1}`}`
    }));
  });

  function columnLabel(i: number): string {
    return detect?.headers[i] || `Column ${i + 1}`;
  }

  function applyDetect(d: CsvDetectResult) {
    detect = d;
    delimiter = d.delimiter;
    hasHeader = d.hasHeader;
    const g = guessMapping(d.headers, d.columnCount);
    dateCol = Math.min(g.dateCol, Math.max(0, d.columnCount - 1));
    descCol = Math.min(g.descCol, Math.max(0, d.columnCount - 1));
    usePair = g.usePair;
    amountCol = g.amountCol !== null ? Math.min(g.amountCol, d.columnCount - 1) : null;
    debitCol = g.debitCol !== null ? Math.min(g.debitCol, d.columnCount - 1) : null;
    creditCol = g.creditCol !== null ? Math.min(g.creditCol, d.columnCount - 1) : null;
  }

  async function handlePickFile() {
    if (picking) return;
    picking = true;
    detectError = null;
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: 'CSV', extensions: ['csv', 'txt', 'tsv'] }]
      });
      if (!picked) return;
      path = picked;
      filename = await basename(picked);
      const d = await csvDetect(picked);
      applyDetect(d);
      preview = null;
      commitResult = null;
      page = 1;
      step = 'mapping';
    } catch (e: unknown) {
      detectError = errMsg(e, 'Failed to read CSV file');
    } finally {
      picking = false;
    }
  }

  function buildMapping() {
    return {
      dateCol,
      descCol,
      amountCol: usePair ? null : amountCol,
      debitCol: usePair ? debitCol : null,
      creditCol: usePair ? creditCol : null,
      numCol: null,
      notesCol: null
    };
  }

  async function handlePreview() {
    if (!path || previewing) return;
    if (!targetAccountGuid || !offsetAccountGuid) {
      previewError = 'Pick both the target and the offset account.';
      return;
    }
    previewing = true;
    previewError = null;
    try {
      const result = await csvPreview({
        path,
        encoding: detect?.encoding ?? null,
        delimiter,
        hasHeader,
        mapping: buildMapping(),
        targetAccountGuid,
        offsetAccountGuid,
        datePreference
      });
      preview = result;
      selected.clear();
      for (const r of result.rows) {
        if (r.valid && !r.duplicate) selected.add(r.key);
      }
      page = 1;
      step = 'preview';
    } catch (e: unknown) {
      previewError = errMsg(e, 'Failed to preview CSV');
    } finally {
      previewing = false;
    }
  }

  let totalPages = $derived(preview ? Math.max(1, Math.ceil(preview.rows.length / pageSize)) : 1);
  let pageRows = $derived.by(() => {
    if (!preview) return [];
    const start = (page - 1) * pageSize;
    return preview.rows.slice(start, start + pageSize);
  });

  let importableKeys = $derived.by(() =>
    (preview?.rows ?? []).filter((r) => r.valid && !r.duplicate).map((r) => r.key)
  );
  let allImportableSelected = $derived.by(
    () => importableKeys.length > 0 && importableKeys.every((k) => selected.has(k))
  );

  function toggleRow(key: number, next: boolean) {
    if (next) selected.add(key);
    else selected.delete(key);
  }

  function toggleAll(next: boolean) {
    selected.clear();
    if (next) {
      for (const k of importableKeys) selected.add(k);
    }
  }

  async function handleCommit() {
    if (!preview || committing || selected.size === 0) return;
    committing = true;
    commitError = null;
    try {
      const result = await csvCommit({
        path,
        encoding: detect?.encoding ?? null,
        delimiter,
        hasHeader,
        mapping: buildMapping(),
        targetAccountGuid,
        offsetAccountGuid,
        datePreference,
        selectedKeys: [...selected]
      });
      commitResult = result;
      step = 'done';
      toast.success(`Imported ${result.created} ${plural(result.created, 'transaction')}`);
      try {
        const home = await fetchHomeData();
        setDbSession(home.summary);
      } catch {
        // Session refresh is best-effort; the ledger refetches on navigation.
      }
    } catch (e: unknown) {
      commitError = errMsg(e, 'CSV import failed');
    } finally {
      committing = false;
    }
  }

  function resetWizard() {
    step = 'file';
    path = '';
    filename = '';
    detect = null;
    preview = null;
    commitResult = null;
    selected.clear();
    page = 1;
    targetAccountGuid = '';
    offsetAccountGuid = '';
    detectError = null;
    previewError = null;
    commitError = null;
  }

  const delimiters = [
    { value: ',', label: 'Comma (,)' },
    { value: ';', label: 'Semicolon (;)' },
    { value: '\t', label: 'Tab' },
    { value: '|', label: 'Pipe (|)' }
  ];
</script>

{#if step === 'file'}
  <Card.Root>
    <Card.Header>
      <Card.Title>Choose a CSV file</Card.Title>
      <Card.Description>
        Bank-statement exports (.csv, .txt, .tsv). Nothing is written until the final import step.
      </Card.Description>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      {#if detectError}
        <ErrorCard message={detectError} onRetry={() => void handlePickFile()} />
      {/if}
      <div>
        <Button onclick={() => void handlePickFile()} disabled={picking}>
          {#if picking}
            <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
            Reading…
          {:else}
            <FileUp aria-hidden="true" class="size-4" />
            Choose CSV file
          {/if}
        </Button>
      </div>
      <p class="text-xs text-muted-foreground">
        Detection runs locally: encoding, delimiter and header row are sniffed before mapping.
      </p>
    </Card.Content>
  </Card.Root>
{:else if step === 'mapping' && detect}
  <div class="flex flex-col gap-4">
    <Card.Root>
      <Card.Header>
        <Card.Title>{filename || 'Map columns'}</Card.Title>
        <Card.Description>
          {detect.rowCount}
          {plural(detect.rowCount, 'data row')} · {detect.columnCount}
          {plural(detect.columnCount, 'column')} · {detect.encoding} · delimiter
          {detect.delimiter === '\t' ? 'tab' : `“${detect.delimiter}”`}
        </Card.Description>
      </Card.Header>
      <Card.Content class="grid gap-4 md:grid-cols-2">
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">Date column</span>
          <Select.Root
            type="single"
            value={String(dateCol)}
            onValueChange={(v) => (dateCol = Number(v))}
            items={columnOptions}
          >
            <Select.Trigger><SelectPrimitive.Value placeholder="Date column" /></Select.Trigger>
            <Select.Content>
              {#each columnOptions as opt (opt.value)}
                <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">Description column</span>
          <Select.Root
            type="single"
            value={String(descCol)}
            onValueChange={(v) => (descCol = Number(v))}
            items={columnOptions}
          >
            <Select.Trigger
              ><SelectPrimitive.Value placeholder="Description column" /></Select.Trigger
            >
            <Select.Content>
              {#each columnOptions as opt (opt.value)}
                <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex items-center gap-2 md:col-span-2">
          <Checkbox
            checked={usePair}
            onCheckedChange={(v) => (usePair = v === true)}
            id="csv-pair"
          />
          <label for="csv-pair" class="text-sm">My file has separate Debit / Credit columns</label>
        </div>
        {#if usePair}
          <div class="flex flex-col gap-2">
            <span class="text-sm font-medium">Debit column (money out)</span>
            <Select.Root
              type="single"
              value={debitCol !== null ? String(debitCol) : ''}
              onValueChange={(v) => (debitCol = Number(v))}
              items={columnOptions}
            >
              <Select.Trigger><SelectPrimitive.Value placeholder="Debit column" /></Select.Trigger>
              <Select.Content>
                {#each columnOptions as opt (opt.value)}
                  <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
          <div class="flex flex-col gap-2">
            <span class="text-sm font-medium">Credit column (money in)</span>
            <Select.Root
              type="single"
              value={creditCol !== null ? String(creditCol) : ''}
              onValueChange={(v) => (creditCol = Number(v))}
              items={columnOptions}
            >
              <Select.Trigger><SelectPrimitive.Value placeholder="Credit column" /></Select.Trigger>
              <Select.Content>
                {#each columnOptions as opt (opt.value)}
                  <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        {:else}
          <div class="flex flex-col gap-2 md:col-span-2">
            <span class="text-sm font-medium">Amount column</span>
            <Select.Root
              type="single"
              value={amountCol !== null ? String(amountCol) : ''}
              onValueChange={(v) => (amountCol = Number(v))}
              items={columnOptions}
            >
              <Select.Trigger><SelectPrimitive.Value placeholder="Amount column" /></Select.Trigger>
              <Select.Content>
                {#each columnOptions as opt (opt.value)}
                  <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        {/if}
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">Target account (statement)</span>
          <AccountPicker
            bind:value={targetAccountGuid}
            options={pickerOptions}
            placeholder="Money in/out of…"
          />
        </div>
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">Offset account (category)</span>
          <AccountPicker
            bind:value={offsetAccountGuid}
            options={pickerOptions}
            placeholder="Balanced against…"
          />
        </div>
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">Delimiter</span>
          <Select.Root
            type="single"
            value={delimiter}
            onValueChange={(v) => {
              if (v) delimiter = v;
            }}
            items={delimiters}
          >
            <Select.Trigger><SelectPrimitive.Value /></Select.Trigger>
            <Select.Content>
              {#each delimiters as d (d.value)}
                <Select.Item value={d.value} label={d.label}>{d.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex flex-col gap-2">
          <span class="text-sm font-medium">Ambiguous dates (01/02/2026)</span>
          <Select.Root
            type="single"
            value={datePreference}
            onValueChange={(v) => {
              if (v === 'dmy' || v === 'mdy') datePreference = v;
            }}
            items={[
              { value: 'dmy', label: 'Day first (DMY)' },
              { value: 'mdy', label: 'Month first (MDY)' }
            ]}
          >
            <Select.Trigger><SelectPrimitive.Value /></Select.Trigger>
            <Select.Content>
              <Select.Item value="dmy" label="Day first (DMY)">Day first (DMY)</Select.Item>
              <Select.Item value="mdy" label="Month first (MDY)">Month first (MDY)</Select.Item>
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex items-center gap-2 md:col-span-2">
          <Checkbox
            checked={hasHeader}
            onCheckedChange={(v) => (hasHeader = v === true)}
            id="csv-header"
          />
          <label for="csv-header" class="text-sm">First row is a header</label>
        </div>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title>Sample rows</Card.Title>
      </Card.Header>
      <Card.Content>
        <TableShell>
          <Table.Header>
            <Table.Row class="bg-muted/75">
              {#each detect.headers as _h, i (i)}
                <Table.Head class="text-muted-foreground">{columnLabel(i)}</Table.Head>
              {/each}
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each detect.sampleRows as row, i (i)}
              <Table.Row>
                {#each row as cell, j (j)}
                  <Table.Cell class="max-w-56 truncate py-2" title={cell}>{cell || '—'}</Table.Cell>
                {/each}
              </Table.Row>
            {/each}
          </Table.Body>
        </TableShell>
      </Card.Content>
    </Card.Root>

    {#if previewError}
      <ErrorCard message={previewError} onRetry={() => void handlePreview()} />
    {/if}

    <div class="flex items-center gap-2">
      <Button variant="outline" onclick={resetWizard}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Back
      </Button>
      <Button
        onclick={() => void handlePreview()}
        disabled={previewing || !targetAccountGuid || !offsetAccountGuid}
      >
        {#if previewing}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Previewing…
        {:else}
          Preview
          <ArrowRight aria-hidden="true" class="size-4" />
        {/if}
      </Button>
    </div>
  </div>
{:else if step === 'preview' && preview}
  <div class="flex flex-col gap-4">
    <Card.Root>
      <Card.Header>
        <Card.Title>Dry-run preview</Card.Title>
        <Card.Description>
          {preview.validCount} ready · {preview.duplicateCount} duplicates (pre-unchecked) ·
          {preview.errorCount} errors · {selected.size} selected
        </Card.Description>
      </Card.Header>
      <Card.Content>
        <TableShell>
          <Table.Header>
            <Table.Row class="bg-muted/75">
              <Table.Head class="w-10">
                <Checkbox
                  checked={allImportableSelected}
                  indeterminate={!allImportableSelected && selected.size > 0}
                  disabled={importableKeys.length === 0}
                  aria-label={allImportableSelected
                    ? 'Unselect all importable rows'
                    : 'Select all importable rows'}
                  onCheckedChange={(v) => toggleAll(v === true)}
                />
              </Table.Head>
              <Table.Head>Date</Table.Head>
              <Table.Head>Description</Table.Head>
              <Table.Head class="text-right">Amount</Table.Head>
              <Table.Head>Status</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each pageRows as row (row.key)}
              {@const checked = selected.has(row.key)}
              {@const disabled = !row.valid || row.duplicate}
              <Table.Row data-checked={checked}>
                <Table.Cell>
                  <Checkbox
                    {checked}
                    {disabled}
                    aria-label="Select row {row.key + 1}"
                    onCheckedChange={(v) => toggleRow(row.key, v === true)}
                  />
                </Table.Cell>
                <Table.Cell class="whitespace-nowrap">
                  {row.postDate ? formatDate(row.postDate) : '—'}
                </Table.Cell>
                <Table.Cell class="max-w-md truncate font-medium" title={row.description ?? ''}>
                  {row.description || '—'}
                </Table.Cell>
                <Table.Cell class="text-right">
                  {row.amount !== null ? formatCurrency(row.amount, currency) : '—'}
                </Table.Cell>
                <Table.Cell>
                  {#if !row.valid}
                    <Badge variant="destructive" title={row.error ?? ''}>Error</Badge>
                  {:else if row.duplicate}
                    <Badge variant="secondary">Duplicate</Badge>
                  {:else}
                    <Badge variant="outline">Ready</Badge>
                  {/if}
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </TableShell>
        <div class="mt-4">
          <PaginationNav
            bind:page
            {totalPages}
            totalItems={preview.rows.length}
            {pageSize}
            onPageSizeChange={(size) => {
              pageSize = size;
              page = 1;
            }}
          />
        </div>
        {#if preview.errorCount > 0}
          <p class="mt-2 text-xs text-muted-foreground">
            Rows with errors stay disabled. Fix the mapping or the file, then re-run preview.
          </p>
        {/if}
      </Card.Content>
    </Card.Root>

    {#if commitError}
      <ErrorCard message={commitError} onRetry={() => void handleCommit()} />
    {/if}

    <div class="flex items-center gap-2">
      <Button variant="outline" onclick={() => (step = 'mapping')}>
        <ArrowLeft aria-hidden="true" class="size-4" />
        Mapping
      </Button>
      <Button onclick={() => void handleCommit()} disabled={committing || selected.size === 0}>
        {#if committing}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Importing…
        {:else}
          <Upload aria-hidden="true" class="size-4" />
          Import {selected.size}
          {plural(selected.size, 'row')}
        {/if}
      </Button>
    </div>
  </div>
{:else if step === 'done' && commitResult}
  <Card.Root>
    <Card.Header>
      <Card.Title class="flex items-center gap-2">
        <CheckCircle2 aria-hidden="true" class="size-5 text-green-600" />
        Import complete
      </Card.Title>
      <Card.Description>{commitResult.label}</Card.Description>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <p class="text-sm text-muted-foreground">
        Created {commitResult.created}
        {plural(commitResult.created, 'transaction')} · skipped {commitResult.skippedDuplicates} duplicates
        · skipped {commitResult.skippedErrors} errors. Revert from Settings → History if needed.
      </p>
      <div class="flex flex-wrap gap-2">
        <Button href={resolve('/transactions')}>
          <ArrowLeft aria-hidden="true" class="size-4" />
          Back to Transactions
        </Button>
        <Button variant="outline" onclick={resetWizard}>Import another file</Button>
      </div>
    </Card.Content>
  </Card.Root>
{/if}
