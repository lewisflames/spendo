<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Upload, LoaderCircle, CircleAlert } from '@lucide/svelte';
  import { uploadDb } from '$lib/db.remote';
  import { dbSession, clearSession } from '$lib/stores/db.svelte';
  import { cn } from '$lib/utils';

  let formElement = $state<HTMLFormElement | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);

  let isDragging = $state(false);
  let errorMessage = $state<string | null>(null);
  let uploadInitiated = $state(false);
  let pendingFilename = $state<string | null>(null);
  let uploadPhase = $state<'idle' | 'uploading' | 'error'>('idle');

  $effect(() => {
    const r = uploadDb.result;
    if (!r || !uploadInitiated) return;
    if (r.ok) {
      dbSession.value = { token: r.token, summary: r.gnucash, filename: pendingFilename };
      uploadPhase = 'idle';
      uploadInitiated = false;
      pendingFilename = null;
    } else {
      errorMessage = r.error;
      uploadPhase = 'error';
    }
  });

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
  }

  function handleDragEnter(e: DragEvent) {
    e.preventDefault();
    isDragging = true;
  }

  function handleDragLeave(e: DragEvent) {
    if (e.currentTarget && !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) {
      isDragging = false;
    }
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    isDragging = false;
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    if (!validateFile(file)) return;
    if (!fileInput) return;
    const dt = new DataTransfer();
    dt.items.add(file);
    fileInput.files = dt.files;
    submitFile();
  }

  function handleFileSelect() {
    if (!fileInput) return;
    const file = fileInput.files?.[0];
    if (!file) return;
    if (!validateFile(file)) return;
    submitFile();
  }

  function validateFile(file: File): boolean {
    if (!file.name.toLowerCase().endsWith('.gnucash')) {
      uploadPhase = 'error';
      errorMessage = 'Only .gnucash files are accepted.';
      return false;
    }
    return true;
  }

  function submitFile() {
    if (!formElement || !fileInput?.files?.[0]) return;
    pendingFilename = fileInput.files[0].name;
    clearSession();
    uploadInitiated = true;
    uploadPhase = 'uploading';
    errorMessage = null;
    formElement.requestSubmit();
  }

  function resetUpload() {
    uploadPhase = 'idle';
    errorMessage = null;
    formElement?.reset();
  }
</script>

<form bind:this={formElement} {...uploadDb} enctype="multipart/form-data" class="hidden">
  <input
    bind:this={fileInput}
    type="file"
    name="dbFile"
    accept=".gnucash"
    onchange={handleFileSelect}
    aria-label="Choose a GnuCash file to upload"
  />
</form>

{#if uploadPhase === 'uploading'}
  <div class="flex flex-col items-center gap-4 py-8 text-center">
    <LoaderCircle aria-hidden="true" class="size-8 animate-spin text-primary" />
    <p class="text-sm text-muted-foreground">Parsing your GnuCash file...</p>
  </div>
{:else if uploadPhase === 'error'}
  <div class="flex flex-col items-center gap-4 py-4 text-center">
    <CircleAlert aria-hidden="true" class="size-10 text-destructive" />
    <p class="text-sm text-destructive">{errorMessage ?? 'Upload failed'}</p>
    <Button variant="outline" onclick={resetUpload}>Try again</Button>
  </div>
{:else}
  <div
    role="button"
    class={cn(
      'border-dashed cursor-pointer transition-colors rounded-xl border-2 p-6 text-center sm:p-12',
      isDragging && 'border-primary bg-muted/50'
    )}
    tabindex={0}
    aria-label="Upload a GnuCash file"
    onclick={() => fileInput?.click()}
    onkeydown={(e) => e.key === 'Enter' && fileInput?.click()}
    ondragover={handleDragOver}
    ondragenter={handleDragEnter}
    ondragleave={handleDragLeave}
    ondrop={handleDrop}
  >
    <Upload aria-hidden="true" class="mx-auto mb-4 size-10 text-muted-foreground" />
    <p class="text-sm font-medium">Drag & drop your .gnucash file</p>
    <p class="text-xs text-muted-foreground">or click to browse</p>
  </div>
{/if}
