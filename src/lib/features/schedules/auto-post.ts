import { toast } from '$lib/components/ui/sonner';
import { refreshSession } from '$lib/stores/db.svelte';
import { postDueSchedules, type AutoPostResult } from '$lib/tauri';
import { plural } from '$lib/utils/format.svelte';

/** Toast copy for an auto-post run; empty when nothing posted or failed. */
export function autoPostMessages(result: AutoPostResult): { posted?: string; failed?: string } {
  const out: { posted?: string; failed?: string } = {};
  if (result.posted > 0) {
    out.posted = `Posted ${plural(result.posted, 'scheduled transaction')}`;
  }
  if (result.failed.length > 0) {
    const names = result.failed.map((f) => f.name).join(', ');
    out.failed = `Couldn’t auto-post ${names}. Open Scheduled to fix it`;
  }
  return out;
}

let lastBook: string | null = null;

/**
 * Post what auto-post schedules have due, once per opened book. The layout
 * calls this whenever the active book changes (`null` = closed, so reopening
 * the same book posts again). Failures stay quiet: the schedules remain due
 * and show in the sidebar badge, and the next open retries.
 */
export async function autoPostForBook(path: string | null): Promise<void> {
  if (path === lastBook) return;
  lastBook = path;
  if (!path) return;
  try {
    const result = await postDueSchedules();
    const messages = autoPostMessages(result);
    if (messages.posted) toast.success(messages.posted);
    if (messages.failed) toast.error(messages.failed);
    if (result.posted > 0) refreshSession();
  } catch {
    // stays due; the badge and Scheduled page surface it
  }
}
