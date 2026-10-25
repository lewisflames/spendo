<script lang="ts">
  import { BookOpen, X } from '@lucide/svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { Button } from '$lib/components/ui/button';
  import { forgetBook, recentBooks } from '$lib/stores/recent-books.svelte';
  import { bookFilesExist } from '$lib/tauri';
  import { bookDisplayName, bookFolder } from '$lib/utils/book';
  import { formatRelativeTime } from '$lib/utils/format.svelte';

  let { disabled, onOpen }: { disabled: boolean; onOpen: (path: string) => void } = $props();

  // Paths whose file is gone. A failed check leaves every book clickable;
  // opening one that vanished meanwhile still toasts the backend's error.
  let missing = $state<string[]>([]);

  $effect(() => {
    const paths = recentBooks.value.map((b) => b.path);
    let stale = false;
    bookFilesExist(paths)
      .then((flags) => {
        if (!stale) missing = paths.filter((_, i) => !flags[i]);
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  });
</script>

<ul class="-mx-2 flex flex-col">
  {#each recentBooks.value as book (book.path)}
    {@const gone = missing.includes(book.path)}
    {@const name = bookDisplayName(book.path)}
    <li class="group flex items-center gap-1 rounded-lg pr-1 hover:bg-accent">
      <button
        type="button"
        class="flex min-w-0 flex-1 items-center gap-3 rounded-lg px-2 py-2 text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none disabled:cursor-not-allowed"
        disabled={disabled || gone}
        onclick={() => onOpen(book.path)}
      >
        <span
          class="grid size-8 shrink-0 place-items-center rounded-md bg-muted text-muted-foreground"
        >
          <BookOpen aria-hidden="true" class="size-4" />
        </span>
        <span class="flex min-w-0 flex-1 flex-col">
          <span class={['truncate text-sm font-medium', gone && 'text-muted-foreground']}>
            {name}
          </span>
          <span
            class="truncate font-mono text-xs text-muted-foreground"
            {@attach tooltip(book.path, { truncated: true })}
          >
            {gone ? 'File not found' : (bookFolder(book.path) ?? book.path)}
          </span>
        </span>
        <span class="shrink-0 text-xs text-muted-foreground">
          {formatRelativeTime(book.openedAt)}
        </span>
      </button>
      <Button
        variant="ghost"
        size="icon-sm"
        class={[
          'shrink-0 text-muted-foreground focus-visible:opacity-100',
          !gone && 'opacity-0 group-hover:opacity-100'
        ]}
        aria-label={`Remove ${name} from recent books`}
        onclick={() => forgetBook(book.path)}
        {@attach tooltip('Remove from list')}
      >
        <X aria-hidden="true" />
      </Button>
    </li>
  {/each}
</ul>
