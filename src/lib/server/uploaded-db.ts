import { writeFile, unlink } from 'node:fs/promises';
import { existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import crypto from 'node:crypto';
import { importGnucash, type ImportResult } from './import-gnucash';

const UPLOAD_DIR = join(tmpdir(), 'spendo-uploads');
const MAX_FILE_SIZE = 100 * 1024 * 1024;
const ALLOWED_EXTENSIONS = ['.gnucash'];

if (!existsSync(UPLOAD_DIR)) {
  mkdirSync(UPLOAD_DIR, { recursive: true });
}

export interface TableInfo {
  name: string;
  sql: string;
  rowCount: number;
}

export async function saveUploadedDb(
  file: File
): Promise<{ tables: TableInfo[]; importResult: ImportResult }> {
  const ext = file.name.toLowerCase().slice(file.name.lastIndexOf('.'));
  if (!ALLOWED_EXTENSIONS.includes(ext)) {
    throw new Error('Unsupported file type. Only .gnucash files are accepted.');
  }
  if (file.size > MAX_FILE_SIZE) {
    throw new Error(`File too large. Maximum size is ${MAX_FILE_SIZE / 1024 / 1024} MB`);
  }

  const token = crypto.randomBytes(16).toString('hex');
  const tempPath = join(UPLOAD_DIR, `${token}${ext}`);

  const buffer = Buffer.from(await file.arrayBuffer());
  await writeFile(tempPath, buffer);

  // Import into app DB, then delete temp file
  try {
    const importResult = importGnucash(tempPath, file.name);
    return { tables: [], importResult };
  } finally {
    try {
      await unlink(tempPath);
    } catch {
      /* ignore */
    }
  }
}
