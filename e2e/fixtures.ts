import type { Page } from '@playwright/test';

/** Canned command results keyed by command name; values must be JSON-serializable. */
export type BackendMocks = Record<string, unknown>;

/**
 * Stub the Tauri IPC bridge before the app boots. Mocked commands resolve with
 * their value; anything else rejects with a `BackendError` naming the command,
 * so a spec fails loudly instead of hanging on a missing backend.
 */
export async function mockBackend(page: Page, mocks: BackendMocks): Promise<void> {
  await page.addInitScript((mocks: BackendMocks) => {
    const callbacks = new Map<number, (data: unknown) => void>();
    let nextId = 1;
    Object.assign(window, {
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        invoke: async (cmd: string) => {
          if (Object.hasOwn(mocks, cmd)) return structuredClone(mocks[cmd]);
          throw { kind: 'internal', message: `unmocked command: ${cmd}` };
        },
        transformCallback: (cb: (data: unknown) => void) => {
          const id = nextId++;
          callbacks.set(id, cb);
          return id;
        },
        unregisterCallback: (id: number) => callbacks.delete(id),
        runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data)
      },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} }
    });
  }, mocks);
}

/** Mocks for a session with no book open (the welcome screen). */
export const NO_BOOK: BackendMocks = {
  fetch_home_data: { hasData: false, summary: null },
  fetch_active_book: { path: null }
};

export { expect, test } from '@playwright/test';
