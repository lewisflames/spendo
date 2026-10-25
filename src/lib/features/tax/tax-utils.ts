import type { AccountNode, TaxRole, TaxRoleAssignment, TaxSummary } from '$lib/tauri';

interface RoleMeta {
  label: string;
  hint: string;
  accountType: 'INCOME' | 'EXPENSE';
}

/** Mirrors `TaxRole` in `tax.rs`, in the order the role picker lists them. */
export const TAX_ROLES: Record<TaxRole, RoleMeta> = {
  employment_income: {
    label: 'Employment income',
    hint: 'Gross wages, before withholding',
    accountType: 'INCOME'
  },
  savings_income: {
    label: 'Savings income',
    hint: 'Interest and dividends, gross',
    accountType: 'INCOME'
  },
  exempt: { label: 'Exempt', hint: 'Income that isn’t taxed, like gifts', accountType: 'INCOME' },
  employment_withholding: {
    label: 'Withholding on wages',
    hint: 'Income tax taken from your pay',
    accountType: 'EXPENSE'
  },
  social_contributions: {
    label: 'Social contributions',
    hint: 'Your social security share',
    accountType: 'EXPENSE'
  },
  savings_withholding: {
    label: 'Withholding on savings',
    hint: 'Tax kept from interest and dividends',
    accountType: 'EXPENSE'
  },
  tax_settlement: {
    label: 'Annual return',
    hint: 'Payments and refunds of the yearly return, counted for the year before',
    accountType: 'EXPENSE'
  }
};

export function rolesFor(accountType: string): TaxRole[] {
  return (Object.keys(TAX_ROLES) as TaxRole[]).filter(
    (r) => TAX_ROLES[r].accountType === accountType
  );
}

/** An income or expense account as the role picker lists it, in tree order. */
export interface RoleRow {
  guid: string;
  name: string;
  code: string | null;
  accountType: 'INCOME' | 'EXPENSE';
  parentGuid: string | null;
  depth: number;
}

/** Income and expense accounts in tree order, depth relative to their top level. */
export function roleRows(trees: AccountNode[] | null | undefined): RoleRow[] {
  const out: RoleRow[] = [];
  function walk(nodes: AccountNode[], parentGuid: string | null, depth: number) {
    for (const n of nodes) {
      const typed = n.accountType === 'INCOME' || n.accountType === 'EXPENSE';
      if (typed) {
        out.push({
          guid: n.guid,
          name: n.name,
          code: n.code,
          accountType: n.accountType as RoleRow['accountType'],
          parentGuid,
          depth
        });
      }
      walk(n.children, typed ? n.guid : null, typed ? depth + 1 : depth);
    }
  }
  walk(trees ?? [], null, 0);
  return out;
}

/** The role an account gets from its nearest ancestor with one, if any. */
export function inheritedRole(
  guid: string,
  rows: readonly RoleRow[],
  roles: ReadonlyMap<string, TaxRole>
): TaxRole | null {
  const byGuid = new Map(rows.map((r) => [r.guid, r]));
  const seen = new Set<string>();
  let parent = byGuid.get(guid)?.parentGuid ?? null;
  while (parent && !seen.has(parent)) {
    seen.add(parent);
    const role = roles.get(parent);
    if (role) return role;
    parent = byGuid.get(parent)?.parentGuid ?? null;
  }
  return null;
}

/** Explicit picks only, dropping roles for accounts no longer listed. */
export function toAssignments(
  roles: ReadonlyMap<string, TaxRole>,
  rows: readonly RoleRow[]
): TaxRoleAssignment[] {
  const listed = new Set(rows.map((r) => r.guid));
  return [...roles]
    .filter(([guid]) => listed.has(guid))
    .map(([accountGuid, role]) => ({ accountGuid, role }));
}

/** Whether the summary has anything set up to show. */
export function hasTaxData(s: TaxSummary): boolean {
  return (
    s.employmentIncome.lines.length +
      s.employmentWithholding.lines.length +
      s.socialContributions.lines.length +
      s.savingsIncome.lines.length +
      s.savingsWithholding.lines.length +
      s.settlement.lines.length +
      s.exempt.lines.length >
      0 || s.realized.disposals > 0
  );
}

/** First and last day of a calendar year, for ledger and list links. */
export function yearRange(year: number): { from: string; to: string } {
  return { from: `${year}-01-01`, to: `${year}-12-31` };
}
