<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import { SvelteSet } from 'svelte/reactivity';
  import { tick } from 'svelte';
  import { toast } from '$lib/components/ui/sonner';
  import {
    csvCommit,
    csvDetect,
    csvPreview,
    deleteCsvPreset,
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
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { guessMapping } from '$lib/features/csv-import/guess';
  import FileStep from '$lib/features/csv-import/steps/file-step.svelte';
  import MappingStep from '$lib/features/csv-import/steps/mapping-step.svelte';
  import PreviewStep from '$lib/features/csv-import/steps/preview-step.svelte';
  import DoneStep from '$lib/features/csv-import/steps/done-step.svelte';

  type Step = 'file' | 'mapping' | 'preview' | 'done';

  const STEPS: { id: Step; label: string }[] = [
    { id: 'file', label: 'File' },
    { id: 'mapping', label: 'Map' },
    { id: 'preview', label: 'Preview' },
    { id: 'done', label: 'Done' }
  ];

  let step = $state<Step>('file');
  let stepIndex = $derived(STEPS.findIndex((s) => s.id === step));
  let stepRoot = $state<HTMLElement | null>(null);
  let stepsMounted = false;

  // Move focus to the incoming step so keyboard/screen-reader users land in
  // the new content instead of on the button that disappeared.
  $effect(() => {
    void step;
    if (!stepsMounted) {
      stepsMounted = true;
      return;
    }
    void tick().then(() => stepRoot?.focus());
  });
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
  let numCol = $state<number | null>(null);
  let notesCol = $state<number | null>(null);

  let signature = $state<string | null>(null);
  let presetApplied = $state(false);
  let rememberMapping = $state(true);
  let presetAccounts = $state<{ target: string; offset: string } | null>(null);

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

  const targetCommodity = $derived(
    targetAccountGuid
      ? (flattenAccountsForPicker(accountsData?.trees, { hidePlaceholders: true }).find(
          (a) => a.guid === targetAccountGuid
        )?.commodity ?? null)
      : null
  );

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
      accountsError = errMsg(e, 'We couldn’t load your accounts');
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
    const maxCol = Math.max(0, d.columnCount - 1);
    const clamp = (i: number) => Math.min(i, maxCol);
    detect = d;
    delimiter = d.delimiter;
    hasHeader = d.hasHeader;
    const g = guessMapping(d.headers, d.columnCount);
    dateCol = clamp(g.dateCol);
    descCol = clamp(g.descCol);
    usePair = g.usePair;
    amountCol = g.amountCol !== null ? clamp(g.amountCol) : null;
    debitCol = g.debitCol !== null ? clamp(g.debitCol) : null;
    creditCol = g.creditCol !== null ? clamp(g.creditCol) : null;
    numCol = g.numCol !== null ? clamp(g.numCol) : null;
    notesCol = g.notesCol !== null ? clamp(g.notesCol) : null;
    signature = d.signature;
    presetApplied = false;
    presetAccounts = null;
    const p = d.preset;
    if (p) {
      delimiter = p.delimiter;
      hasHeader = p.hasHeader;
      datePreference = p.datePreference === 'mdy' ? 'mdy' : 'dmy';
      usePair = p.mapping.debitCol != null || p.mapping.creditCol != null;
      amountCol = p.mapping.amountCol ?? null;
      debitCol = p.mapping.debitCol ?? null;
      creditCol = p.mapping.creditCol ?? null;
      numCol = p.mapping.numCol ?? null;
      notesCol = p.mapping.notesCol ?? null;
      presetAccounts = { target: p.targetAccountGuid, offset: p.offsetAccountGuid };
      presetApplied = true;
    }
  }

  // Apply the saved preset’s accounts once the tree is available. The picker
  // only accepts real account guids, so the check also drops stale presets.
  $effect(() => {
    if (!presetAccounts || !accountsData) return;
    const guids = new Set(
      flattenAccountsForPicker(accountsData.trees, { hidePlaceholders: true }).map((a) => a.guid)
    );
    if (guids.has(presetAccounts.target)) targetAccountGuid = presetAccounts.target;
    if (guids.has(presetAccounts.offset)) offsetAccountGuid = presetAccounts.offset;
    presetAccounts = null;
  });

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
      detectError = errMsg(e, 'We couldn’t read that CSV file');
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
      numCol,
      notesCol
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
        fileSha256: detect?.sha256 ?? '',
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
      previewError = errMsg(e, 'We couldn’t preview that CSV');
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
        fileSha256: detect?.sha256 ?? '',
        encoding: detect?.encoding ?? null,
        delimiter,
        hasHeader,
        mapping: buildMapping(),
        targetAccountGuid,
        offsetAccountGuid,
        datePreference,
        selectedKeys: [...selected],
        rememberMapping
      });
      commitResult = result;
      step = 'done';
      toast.success(`Brought in ${plural(result.created, 'transaction')}`);
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

  async function forgetPreset() {
    if (!signature) return;
    try {
      await deleteCsvPreset(signature);
      presetApplied = false;
      toast.success('Mapping forgotten. We’ll ask next time');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t forget that mapping. Please try again'));
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
    numCol = null;
    notesCol = null;
    signature = null;
    presetApplied = false;
    rememberMapping = true;
    presetAccounts = null;
  }
</script>

<div bind:this={stepRoot} tabindex="-1" class="flex flex-col gap-6 outline-none">
  <ol class="flex flex-wrap items-center gap-2 text-sm" aria-label="Import steps">
    {#each STEPS as s, i (s.id)}
      <li class="flex items-center gap-2" aria-current={i === stepIndex ? 'step' : undefined}>
        <span
          class="flex size-5 items-center justify-center rounded-full text-xs font-medium {i <=
          stepIndex
            ? 'bg-primary text-primary-foreground'
            : 'bg-muted text-muted-foreground'}"
        >
          {i + 1}
        </span>
        <span class={i === stepIndex ? 'font-medium' : 'text-muted-foreground'}>{s.label}</span>
      </li>
    {/each}
  </ol>

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
      bind:numCol
      bind:notesCol
      bind:targetAccountGuid
      bind:offsetAccountGuid
      bind:rememberMapping
      {accountsData}
      {accountsError}
      {previewError}
      {previewing}
      {presetApplied}
      hasSignature={signature !== null}
      onForgetPreset={() => void forgetPreset()}
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
      {targetCommodity}
      onCommit={() => void handleCommit()}
      onBack={() => (step = 'mapping')}
    />
  {:else if step === 'done' && commitResult}
    <DoneStep {commitResult} onReset={resetWizard} />
  {/if}
</div>
