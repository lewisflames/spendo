<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { SvelteSet } from 'svelte/reactivity';
  import { toast } from '$lib/components/ui/sonner';
  import {
    csvCommit,
    csvDetect,
    csvPreview,
    fetchAccounts,
    fetchHomeData,
    type AccountsResult,
    type CsvCommitResult,
    type CsvDetectResult,
    type CsvPreviewResult
  } from '$lib/tauri';
  import { dbSession, setDbSession } from '$lib/stores/db.svelte';
  import { errMsg } from '$lib/utils/error';
  import { plural } from '$lib/utils/format.svelte';
  import { guessMapping } from '$lib/features/csv-import/guess';
  import FileStep from '$lib/features/csv-import/steps/file-step.svelte';
  import MappingStep from '$lib/features/csv-import/steps/mapping-step.svelte';
  import PreviewStep from '$lib/features/csv-import/steps/preview-step.svelte';
  import DoneStep from '$lib/features/csv-import/steps/done-step.svelte';

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
  // Local preview pagination (deliberately not the global pageSize: the wizard
  // shows dense 50-row dry-run pages independent of ledger preferences).
  let pageSize = $state(50);

  let picking = $state(false);
  let previewing = $state(false);
  let committing = $state(false);
  let detectError = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let commitError = $state<string | null>(null);
  let commitResult = $state<CsvCommitResult | null>(null);

  let accountsData = $state<AccountsResult | null>(null);
  let accountsError = $state<string | null>(null);

  // Load account trees once a book is present (for both pickers). A failed
  // load surfaces inline: without an error the pickers just stay empty and
  // Preview looks inexplicably disabled.
  let accountsRequest = 0;
  async function loadAccounts() {
    const request = ++accountsRequest;
    accountsError = null;
    try {
      const r = await fetchAccounts();
      if (request !== accountsRequest) return;
      accountsData = r;
    } catch (e: unknown) {
      if (request !== accountsRequest) return;
      accountsData = null;
      accountsError = errMsg(e, 'Failed to load accounts');
    }
  }

  $effect(() => {
    if (!dbSession.value) return;
    void loadAccounts();
    return () => {
      accountsRequest++;
    };
  });

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
      toast.success(`Imported ${plural(result.created, 'transaction')}`);
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
</script>

{#if step === 'file'}
  <FileStep {detectError} {picking} onDetect={() => void handlePickFile()} />
{:else if step === 'mapping' && detect}
  <MappingStep
    {detect}
    {filename}
    bind:delimiter
    bind:hasHeader
    bind:datePreference
    bind:usePair
    bind:dateCol
    bind:descCol
    bind:amountCol
    bind:debitCol
    bind:creditCol
    bind:targetAccountGuid
    bind:offsetAccountGuid
    {accountsData}
    {accountsError}
    {previewError}
    {previewing}
    onPreview={() => void handlePreview()}
    onReset={resetWizard}
    onRetryAccounts={() => void loadAccounts()}
  />
{:else if step === 'preview' && preview}
  <PreviewStep
    {preview}
    {selected}
    bind:page
    bind:pageSize
    {commitError}
    {committing}
    onCommit={() => void handleCommit()}
    onBack={() => (step = 'mapping')}
  />
{:else if step === 'done' && commitResult}
  <DoneStep {commitResult} onReset={resetWizard} />
{/if}
