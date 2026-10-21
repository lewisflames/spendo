import { BundleType, getBundleType } from '@tauri-apps/api/app';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { errMsg } from '$lib/utils/error';

type UpdateStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'downloading' | 'error';

/**
 * The macOS app and the Linux AppImage install their own updates. The deb
 * (repackaged as the Arch AUR `spendo-bin`) belongs to the package manager.
 */
const SELF_UPDATING_BUNDLES: readonly BundleType[] = [BundleType.App, BundleType.AppImage];

async function detectSelfUpdate(): Promise<boolean> {
  // Unbundled dev builds report no type, and older runtimes may reject.
  const bundle = await getBundleType().catch(() => null);
  return bundle !== null && SELF_UPDATING_BUNDLES.includes(bundle);
}

export const updates = $state<{
  value: {
    status: UpdateStatus;
    version: string | null;
    notes: string | null;
    progress: number | null;
    error: string | null;
    /** Whether this install can download and apply `version` itself. */
    canSelfUpdate: boolean;
  };
}>({
  value: {
    status: 'idle',
    version: null,
    notes: null,
    progress: null,
    error: null,
    canSelfUpdate: false
  }
});

/** Checked but not installed update; it holds a resource until `close()`. */
let pending: Update | null = null;

function resetResult() {
  updates.value.version = null;
  updates.value.notes = null;
  updates.value.progress = null;
  updates.value.error = null;
}

export async function checkForUpdates(): Promise<void> {
  const { status } = updates.value;
  if (status === 'checking' || status === 'downloading') return;

  // Drop a previous result before asking again so its resource isn't leaked.
  if (pending) {
    const previous = pending;
    pending = null;
    await previous.close().catch(() => {});
  }
  resetResult();
  updates.value.status = 'checking';

  try {
    const [update, canSelfUpdate] = await Promise.all([check(), detectSelfUpdate()]);
    updates.value.canSelfUpdate = canSelfUpdate;
    if (!update) {
      updates.value.status = 'up-to-date';
      return;
    }
    pending = update;
    updates.value.version = update.version;
    updates.value.notes = update.body ?? null;
    updates.value.status = 'available';
  } catch (e) {
    updates.value.error = errMsg(e, 'We couldn’t check for updates. Please try again.');
    updates.value.status = 'error';
  }
}

export async function installUpdate(): Promise<void> {
  if (!pending || !updates.value.canSelfUpdate || updates.value.status === 'downloading') return;
  const update = pending;

  updates.value.status = 'downloading';
  updates.value.progress = 0;
  updates.value.error = null;

  let total: number | null = null;
  let received = 0;

  try {
    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength ?? null;
        received = 0;
        updates.value.progress = total === null ? null : 0;
      } else if (event.event === 'Progress') {
        received += event.data.chunkLength;
        updates.value.progress = total === null ? null : Math.min(received / total, 1);
      } else {
        updates.value.progress = 1;
      }
    });
    await relaunch();
  } catch (e) {
    updates.value.error = errMsg(e, 'We couldn’t install the update. Please try again.');
    updates.value.status = 'error';
  }
}
