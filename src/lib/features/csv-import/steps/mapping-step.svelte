<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Select from '$lib/components/ui/select';
  import * as Table from '$lib/components/ui/table';
  import { Select as SelectPrimitive } from 'bits-ui';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import { AccountPicker } from '$lib/components/shared/account-picker';
  import { ArrowLeft, ArrowRight, LoaderCircle } from '@lucide/svelte';
  import type { AccountsResult, CsvDetectResult } from '$lib/tauri';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { plural } from '$lib/utils/format.svelte';

  let {
    detect,
    filename,
    delimiter = $bindable(),
    hasHeader = $bindable(),
    datePreference = $bindable(),
    usePair = $bindable(),
    dateCol = $bindable(),
    descCol = $bindable(),
    amountCol = $bindable(),
    debitCol = $bindable(),
    creditCol = $bindable(),
    numCol = $bindable<number | null>(),
    notesCol = $bindable<number | null>(),
    targetAccountGuid = $bindable(),
    offsetAccountGuid = $bindable(),
    rememberMapping = $bindable(),
    accountsData,
    accountsError,
    previewError,
    previewing,
    presetApplied,
    hasSignature,
    onForgetPreset,
    onPreview,
    onReset,
    onRetryAccounts
  }: {
    detect: CsvDetectResult;
    filename: string;
    delimiter: string;
    hasHeader: boolean;
    datePreference: 'dmy' | 'mdy';
    usePair: boolean;
    dateCol: number;
    descCol: number;
    amountCol: number | null;
    debitCol: number | null;
    creditCol: number | null;
    numCol: number | null;
    notesCol: number | null;
    targetAccountGuid: string;
    offsetAccountGuid: string;
    rememberMapping: boolean;
    accountsData: AccountsResult | null;
    accountsError: string | null;
    previewError: string | null;
    previewing: boolean;
    presetApplied: boolean;
    hasSignature: boolean;
    onForgetPreset: () => void;
    onPreview: () => void;
    onReset: () => void;
    onRetryAccounts: () => void;
  } = $props();

  let pickerOptions = $derived(
    flattenAccountsForPicker(accountsData?.trees, { hidePlaceholders: true })
  );

  let columnOptions = $derived.by(() => {
    const headers = detect.headers ?? [];
    const count = detect.columnCount ?? 0;
    return Array.from({ length: count }, (_, i) => ({
      value: String(i),
      label: `${i + 1}. ${headers[i] || `Column ${i + 1}`}`
    }));
  });

  function columnLabel(i: number): string {
    return detect.headers[i] || `Column ${i + 1}`;
  }

  const delimiters = [
    { value: ',', label: 'Comma (,)' },
    { value: ';', label: 'Semicolon (;)' },
    { value: '\t', label: 'Tab' },
    { value: '|', label: 'Pipe (|)' }
  ];
</script>

<div class="flex flex-col gap-4">
  <Card.Root>
    <Card.Header>
      <Card.Title>{filename || 'Map columns'}</Card.Title>
      <Card.Description>
        {plural(detect.rowCount, 'data row')} · {plural(detect.columnCount, 'column')} · {detect.encoding}
        · delimiter
        {detect.delimiter === '\t' ? 'tab' : `“${detect.delimiter}”`}
      </Card.Description>
    </Card.Header>
    <Card.Content class="grid gap-4 md:grid-cols-2">
      {#if presetApplied}
        <div class="flex items-center gap-2 md:col-span-2">
          <p class="text-sm text-muted-foreground">We reused your saved mapping for this layout.</p>
          <Button variant="ghost" size="sm" onclick={onForgetPreset}>Forget</Button>
        </div>
      {/if}
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
          <Select.Trigger><SelectPrimitive.Value placeholder="Description column" /></Select.Trigger
          >
          <Select.Content>
            {#each columnOptions as opt (opt.value)}
              <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center gap-2 md:col-span-2">
        <Checkbox checked={usePair} onCheckedChange={(v) => (usePair = v === true)} id="csv-pair" />
        <label for="csv-pair" class="text-sm"
          >This file splits money in and out into two columns</label
        >
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
        <span class="text-sm font-medium">Reference / number (optional)</span>
        <Select.Root
          type="single"
          value={numCol !== null ? String(numCol) : 'none'}
          onValueChange={(v) => (numCol = v === 'none' ? null : Number(v))}
          items={[{ value: 'none', label: 'Not imported' }, ...columnOptions]}
        >
          <Select.Trigger><SelectPrimitive.Value /></Select.Trigger>
          <Select.Content>
            <Select.Item value="none" label="Not imported">Not imported</Select.Item>
            {#each columnOptions as opt (opt.value)}
              <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-2">
        <span class="text-sm font-medium">Notes / memo (optional)</span>
        <Select.Root
          type="single"
          value={notesCol !== null ? String(notesCol) : 'none'}
          onValueChange={(v) => (notesCol = v === 'none' ? null : Number(v))}
          items={[{ value: 'none', label: 'Not imported' }, ...columnOptions]}
        >
          <Select.Trigger><SelectPrimitive.Value /></Select.Trigger>
          <Select.Content>
            <Select.Item value="none" label="Not imported">Not imported</Select.Item>
            {#each columnOptions as opt (opt.value)}
              <Select.Item value={opt.value} label={opt.label}>{opt.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-2">
        <span class="text-sm font-medium">Which account is this statement for?</span>
        <AccountPicker
          bind:value={targetAccountGuid}
          options={pickerOptions}
          placeholder="Money in/out of…"
        />
      </div>
      <div class="flex flex-col gap-2">
        <span class="text-sm font-medium">Fallback category (when no rule matches)</span>
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
      {#if hasSignature}
        <div class="flex items-center gap-2 md:col-span-2">
          <Checkbox
            checked={rememberMapping}
            onCheckedChange={(v) => (rememberMapping = v === true)}
            id="csv-remember"
          />
          <label for="csv-remember" class="text-sm"
            >Remember this for next time with this bank</label
          >
        </div>
      {/if}
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
                <Table.Cell class="max-w-56 truncate py-2" title={cell}>
                  {#if cell}
                    {cell}
                  {:else}
                    <EmptyValue />
                  {/if}
                </Table.Cell>
              {/each}
            </Table.Row>
          {/each}
        </Table.Body>
      </TableShell>
    </Card.Content>
  </Card.Root>

  {#if previewError}
    <ErrorCard message={previewError} onRetry={onPreview} />
  {/if}

  {#if accountsError}
    <ErrorCard message={accountsError} onRetry={onRetryAccounts} />
  {/if}

  <div class="flex items-center gap-2">
    <Button variant="outline" onclick={onReset}>
      <ArrowLeft aria-hidden="true" class="size-4" />
      Back
    </Button>
    <Button onclick={onPreview} disabled={previewing || !targetAccountGuid || !offsetAccountGuid}>
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
