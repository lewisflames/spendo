import { query } from '$app/server';
import { getAccountDetail, type AccountDetailResult } from '$lib/server/account-detail';

export const fetchAccountDetail = query(
  'unchecked',
  async ({
    token,
    guid,
    page,
    pageSize
  }: {
    token: string;
    guid: string;
    page: number;
    pageSize: number;
  }): Promise<AccountDetailResult> => {
    return getAccountDetail(token, guid, page, pageSize);
  }
);
