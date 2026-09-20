<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Badge } from '$lib/components/ui/badge';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ImportRuleDialog from './import-rule-dialog.svelte';
  import {
    deleteImportRule,
    fetchAccounts,
    fetchImportRules,
    updateImportRule,
    type ImportRule
  } from '$lib/tauri';
  import { flattenAccountsForPicker } from '$lib/features/accounts/account-picker-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { errMsg } from '$lib/utils/error';
  import { plural } from '$lib/utils/format.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { ArrowDown, ArrowUp, ListFilter, Pencil, Plus, Trash2 } from '@lucide/svelte';

  const rulesRes = useAsyncData({
    enabled: () => true,
    fetch: fetchImportRules,
    label: 'Failed to load import rules'
  });

  const accountsRes = useAsyncData({
    enabled: () => true,
    fetch: fetchAccounts,
    label: 'Failed to load accounts'
  });

  const rules = $derived(rulesRes.data ?? []);
  const pickerOptions = $derived(
    flattenAccountsForPicker(accountsRes.data?.trees, { hidePlaceholders: true })
  );

  let dialogOpen = $state(false);
  let editingRule = $state<ImportRule | null>(null);
  let movingId = $state<number | null>(null);
  let deletingId = $state<number | null>(null);
  let confirmDeleteId = $state<number | null>(null);

  function openCreate() {
    editingRule = null;
    dialogOpen = true;
  }

  function openEdit(rule: ImportRule) {
    editingRule = rule;
    dialogOpen = true;
  }

  function ruleInput(rule: ImportRule, priority: number) {
    return {
      priority,
      matchType: rule.matchType,
      pattern: rule.pattern,
      accountGuid: rule.accountGuid
    };
  }

  async function moveRule(index: number, direction: -1 | 1) {
    if (movingId !== null || deletingId !== null) return;
    const current = rules[index];
    const adjacent = rules[index + direction];
    if (!current || !adjacent) return;
    // Equal priorities fall back to the id tie-break, so a swap wouldn't move.
    if (current.priority === adjacent.priority) return;
    movingId = current.id;
    try {
      await updateImportRule(current.id, ruleInput(current, adjacent.priority));
      await updateImportRule(adjacent.id, ruleInput(adjacent, current.priority));
      rulesRes.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to reorder import rules'));
    } finally {
      movingId = null;
    }
  }

  async function handleDelete(rule: ImportRule) {
    if (deletingId !== null) return;
    if (confirmDeleteId !== rule.id) {
      confirmDeleteId = rule.id;
      return;
    }
    deletingId = rule.id;
    try {
      await deleteImportRule(rule.id);
      confirmDeleteId = null;
      rulesRes.reload();
      toast.success('Import rule deleted');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to delete the rule'));
    } finally {
      deletingId = null;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Import rules"
    description="CSV rows whose description matches a rule post to that category; the mapping step's account is the fallback."
  >
    {#snippet icon()}
      <ListFilter aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3">
    <div class="flex items-center justify-between gap-3">
      <p class="text-xs text-muted-foreground">
        {#if rules.length > 0}
          {plural(rules.length, 'rule')}
        {:else}
          No rules yet
        {/if}
      </p>
      <Button variant="outline" size="sm" onclick={openCreate}>
        <Plus aria-hidden="true" class="size-4" />
        Add rule
      </Button>
    </div>

    {#if rulesRes.loading && !rulesRes.data}
      <p class="py-2 text-sm text-muted-foreground">Loading rules…</p>
    {:else if rulesRes.error}
      <ErrorCard message={rulesRes.error} onRetry={() => rulesRes.reload()} />
    {:else if rules.length > 0}
      <ul class="flex flex-col gap-1.5">
        {#each rules as rule, i (rule.id)}
          <li class="flex items-center gap-2 rounded-lg border px-3 py-2">
            <span class="min-w-0 flex-1 truncate text-sm font-medium" title={rule.pattern}>
              {rule.pattern}
            </span>
            <Badge variant="outline" class="shrink-0">
              {rule.matchType === 'equals' ? 'exact' : 'contains'}
            </Badge>
            <span
              class="max-w-40 min-w-0 truncate text-sm text-muted-foreground"
              title={rule.accountName}
            >
              {rule.accountName}
            </span>
            <div class="flex shrink-0 items-center gap-0.5">
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Move rule up"
                title="Move up"
                disabled={i === 0 || movingId !== null || deletingId !== null}
                onclick={() => void moveRule(i, -1)}
              >
                <ArrowUp aria-hidden="true" class="size-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Move rule down"
                title="Move down"
                disabled={i === rules.length - 1 || movingId !== null || deletingId !== null}
                onclick={() => void moveRule(i, 1)}
              >
                <ArrowDown aria-hidden="true" class="size-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Edit import rule"
                title="Edit"
                disabled={deletingId !== null}
                onclick={() => openEdit(rule)}
              >
                <Pencil aria-hidden="true" class="size-4" />
              </Button>
              {#if confirmDeleteId === rule.id}
                <span class="text-xs text-destructive">Delete?</span>
                <Button
                  variant="destructive"
                  size="sm"
                  onclick={() => void handleDelete(rule)}
                  disabled={deletingId !== null}
                >
                  {deletingId === rule.id ? 'Deleting…' : 'Delete'}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => (confirmDeleteId = null)}
                  disabled={deletingId !== null}
                >
                  Cancel
                </Button>
              {:else}
                <Button
                  variant="ghost"
                  size="icon-sm"
                  class="text-destructive hover:text-destructive"
                  aria-label="Delete import rule"
                  title="Delete"
                  disabled={deletingId !== null}
                  onclick={() => void handleDelete(rule)}
                >
                  <Trash2 aria-hidden="true" class="size-4" />
                </Button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <EmptyState
        title="No import rules"
        description="Add a rule to route CSV rows whose description matches to a category."
      >
        {#snippet icon()}
          <ListFilter aria-hidden="true" />
        {/snippet}
      </EmptyState>
    {/if}
  </Card.Content>
</Card.Root>

<ImportRuleDialog
  bind:open={dialogOpen}
  options={pickerOptions}
  rule={editingRule}
  nextPriority={rules.length + 1}
  onSaved={() => rulesRes.reload()}
/>
