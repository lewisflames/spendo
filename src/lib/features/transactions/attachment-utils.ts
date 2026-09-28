/**
 * Pending attachments for the transaction form. Files picked before the
 * transaction exists are held client-side and attached right after save.
 * The extension allowlist mirrors `ALLOWED_TYPES` in
 * `src-tauri/src/attachments.rs`; the backend re-validates every file when it
 * is actually attached.
 */
export const ATTACHMENT_EXTENSIONS = ['pdf', 'png', 'jpg', 'jpeg', 'gif', 'webp'];

export const MAX_ATTACHMENTS = 10;

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp'];

export interface PendingSelection {
  /** New paths to hold: allowed, deduplicated and within the limit. */
  added: string[];
  /** Rejected paths whose extension the backend does not accept. */
  unsupported: string[];
  /** Allowed, non-duplicate paths that didn't fit under the limit. */
  overflow: number;
}

/** Display name for a path: the final segment, Windows separators included. */
export function attachmentFileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function extensionOf(path: string): string {
  const name = attachmentFileName(path);
  const dot = name.lastIndexOf('.');
  return dot === -1 ? '' : name.slice(dot + 1).toLowerCase();
}

export function isAllowedAttachment(path: string): boolean {
  return ATTACHMENT_EXTENSIONS.includes(extensionOf(path));
}

export function isImageAttachment(path: string): boolean {
  return IMAGE_EXTENSIONS.includes(extensionOf(path));
}

/**
 * Which freshly picked/dropped paths join a pending list: unsupported
 * extensions are reported, duplicates skipped, and the per-transaction limit
 * enforced. Order is preserved.
 */
export function selectPendingAttachments(
  existing: string[],
  incoming: string[],
  max = MAX_ATTACHMENTS
): PendingSelection {
  const unsupported = incoming.filter((path) => !isAllowedAttachment(path));
  const seen = new Set(existing);
  const allowed: string[] = [];
  for (const path of incoming) {
    if (!isAllowedAttachment(path) || seen.has(path)) continue;
    seen.add(path);
    allowed.push(path);
  }
  const room = Math.max(max - existing.length, 0);
  return {
    added: allowed.slice(0, room),
    unsupported,
    overflow: Math.max(allowed.length - room, 0)
  };
}
