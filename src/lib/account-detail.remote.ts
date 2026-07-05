import { z } from 'zod';
import { query } from '$app/server';
import { getAccountDetail, type AccountDetailResult } from '$lib/server/account-detail';

const accountDetailSchema = z.object({
  guid: z.string().min(1),
  page: z.number().int().positive(),
  pageSize: z.number().int().positive(),
  dateFrom: z.string().optional(),
  dateTo: z.string().optional(),
  search: z.string().optional()
});

export const fetchAccountDetail = query(
  accountDetailSchema,
  async ({
    guid,
    page,
    pageSize,
    dateFrom,
    dateTo,
    search
  }: z.infer<typeof accountDetailSchema>): Promise<AccountDetailResult> => {
    return getAccountDetail(guid, page, pageSize, dateFrom, dateTo, search);
  }
);
