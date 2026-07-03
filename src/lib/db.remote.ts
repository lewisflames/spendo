import { form } from '$app/server';
import { saveUploadedDb, type TableInfo } from '$lib/server/uploaded-db';
import type { GnucashSummary } from '$lib/server/gnucash-parser';

type UploadResult =
  | { ok: true; token: string; tables: TableInfo[]; gnucash: GnucashSummary }
  | { ok: false; error: string };

export const uploadDb = form('unchecked', async (data: { dbFile: File }): Promise<UploadResult> => {
  try {
    if (!data.dbFile || !(data.dbFile instanceof File)) {
      return { ok: false, error: 'No file provided' };
    }
    const result = await saveUploadedDb(data.dbFile);
    return { ok: true, ...result };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : 'Upload failed' };
  }
});
