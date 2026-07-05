import { z } from 'zod';
import { form, query } from '$app/server';
import { saveUploadedDb, type TableInfo } from '$lib/server/uploaded-db';
import { getAppDb, clearAppDb as clearAppDbFn } from '$lib/server/app-db';
import { parseGnucashDb, type GnucashSummary } from '$lib/server/gnucash-parser';
import {
  getImportLog,
  getImportLogs,
  deleteImportLog as deleteImportLogFn
} from '$lib/server/import-gnucash';
import type { ImportResult, ImportLogEntry, ImportLogDetails } from '$lib/server/import-gnucash';

export interface HomeDataResult {
  hasData: boolean;
  summary: GnucashSummary | null;
  lastImport: ImportLogEntry | null;
}

type UploadResult =
  { ok: true; tables: TableInfo[]; importResult: ImportResult } | { ok: false; error: string };

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

const noParamsSchema = z.object({}).strict();

export const clearAppDb = query(
  noParamsSchema,
  async (_data: Record<string, never>): Promise<{ cleared: boolean }> => {
    clearAppDbFn();
    return { cleared: true };
  }
);

export const fetchHomeData = query(
  noParamsSchema,
  async (_data: Record<string, never>): Promise<HomeDataResult> => {
    const db = getAppDb();
    try {
      const { cnt } = (db.prepare('SELECT COUNT(*) as cnt FROM accounts').get() ?? { cnt: 0 }) as {
        cnt: number;
      };
      if (cnt === 0) {
        return { hasData: false, summary: null, lastImport: null };
      }
      const summary = parseGnucashDb(db);
      const lastImport = getImportLog();
      return { hasData: true, summary, lastImport };
    } finally {
      db.close();
    }
  }
);

export const fetchImportHistory = query(
  noParamsSchema,
  async (_data: Record<string, never>): Promise<ImportLogDetails[]> => {
    return getImportLogs(20);
  }
);

export const deleteImportLog = query(
  z.object({ id: z.number().int().positive() }).strict(),
  async (data: { id: number }): Promise<{ ok: boolean }> => {
    deleteImportLogFn(data.id);
    return { ok: true };
  }
);
