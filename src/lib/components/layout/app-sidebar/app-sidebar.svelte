<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';

  import {
    ArrowLeftRight,
    Briefcase,
    CalendarClock,
    ChevronsUpDown,
    CircleHelp,
    Copy,
    Database,
    ExternalLink,
    FolderOpen,
    FolderTree,
    Info,
    LogOut,
    PiggyBank,
    Plus,
    Receipt,
    Save,
    Scale,
    Settings,
    Upload
  } from '@lucide/svelte';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import {
    closeActiveBook,
    copyActiveBookPath,
    createBookViaDialog,
    openBookViaDialog,
    revealActiveBook,
    saveBookAsViaDialog
  } from '$lib/features/book/book-actions';
  import { activeBookPath, dbSession } from '$lib/stores/db.svelte';
  import { keyboardShortcutsDialog } from '$lib/stores/ui.svelte';
  import { bookDisplayName } from '$lib/utils/book';
  import { plural } from '$lib/utils/format.svelte';

  const sidebar = Sidebar.useSidebar();

  const bookName = $derived(bookDisplayName(activeBookPath.value));
  const bookSubtitle = $derived(
    dbSession.value
      ? `${dbSession.value.summary.currency} · ${plural(dbSession.value.summary.totalTransactions, 'transaction')}`
      : null
  );
</script>

<Sidebar.Root class="top-(--header-height) h-[calc(100svh-var(--header-height))]!">
  <Sidebar.Content>
    <Sidebar.Group>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/transactions' ||
                page.url.pathname.startsWith('/transactions/')}
              tooltipContent="Transactions"
            >
              {#snippet child({ props })}
                <a href={resolve('/transactions')} {...props}>
                  <ArrowLeftRight aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Transactions</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname.startsWith('/accounts')}
              tooltipContent="Accounts"
            >
              {#snippet child({ props })}
                <a href={resolve('/accounts')} {...props}>
                  <FolderTree aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Accounts</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/scheduled' ||
                page.url.pathname.startsWith('/scheduled/')}
              tooltipContent="Scheduled"
            >
              {#snippet child({ props })}
                <a href={resolve('/scheduled')} {...props}>
                  <CalendarClock aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Scheduled</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/reconcile' ||
                page.url.pathname.startsWith('/reconcile/')}
              tooltipContent="Reconcile"
            >
              {#snippet child({ props })}
                <a href={resolve('/reconcile')} {...props}>
                  <Scale aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Reconcile</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/reports/pnl'}
              tooltipContent="Profit & Loss"
            >
              {#snippet child({ props })}
                <a href={resolve('/reports/pnl')} {...props}>
                  <Receipt aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Profit &amp; Loss</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/budgets' ||
                page.url.pathname.startsWith('/budgets/')}
              tooltipContent="Budgets"
            >
              {#snippet child({ props })}
                <a href={resolve('/budgets')} {...props}>
                  <PiggyBank aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Budgets</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/portfolio' ||
                page.url.pathname.startsWith('/portfolio/')}
              tooltipContent="Portfolio"
            >
              {#snippet child({ props })}
                <a href={resolve('/portfolio')} {...props}>
                  <Briefcase aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Portfolio</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        </Sidebar.Menu>
      </Sidebar.GroupContent>
    </Sidebar.Group>
  </Sidebar.Content>
  <Sidebar.Footer>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Sidebar.MenuButton
                {...props}
                size="lg"
                class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
              >
                <Database aria-hidden="true" />
                <div
                  class="grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden"
                >
                  <span class="truncate font-medium">{bookName}</span>
                  {#if bookSubtitle}
                    <span class="truncate text-xs text-muted-foreground">{bookSubtitle}</span>
                  {/if}
                </div>
                <ChevronsUpDown
                  class="ml-auto group-data-[collapsible=icon]:hidden"
                  aria-hidden="true"
                />
              </Sidebar.MenuButton>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content
            class="w-64"
            align="start"
            side={sidebar.collapsed ? 'right' : 'top'}
          >
            <DropdownMenu.Label>
              <div class="min-w-0">
                <span class="block truncate font-medium">{bookName}</span>
                {#if activeBookPath.value}
                  <span
                    class="block truncate text-xs text-muted-foreground"
                    title={activeBookPath.value}
                  >
                    {activeBookPath.value}
                  </span>
                {/if}
              </div>
            </DropdownMenu.Label>
            <DropdownMenu.Separator />
            <DropdownMenu.Item onclick={() => void openBookViaDialog()}>
              <FolderOpen aria-hidden="true" />
              Open book…
            </DropdownMenu.Item>
            <DropdownMenu.Item onclick={() => void createBookViaDialog()}>
              <Plus aria-hidden="true" />
              New book…
            </DropdownMenu.Item>
            <DropdownMenu.Item onclick={() => void saveBookAsViaDialog()}>
              <Save aria-hidden="true" />
              Save as…
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item onclick={() => void revealActiveBook()}>
              <ExternalLink aria-hidden="true" />
              Reveal in file manager
            </DropdownMenu.Item>
            <DropdownMenu.Item onclick={() => void copyActiveBookPath()}>
              <Copy aria-hidden="true" />
              Copy path
            </DropdownMenu.Item>
            <DropdownMenu.Item>
              {#snippet child({ props })}
                <a href={resolve('/settings/import')} {...props}>
                  <Upload aria-hidden="true" />
                  Import data…
                </a>
              {/snippet}
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item>
              {#snippet child({ props })}
                <a href={resolve('/settings')} {...props}>
                  <Settings aria-hidden="true" />
                  Settings
                </a>
              {/snippet}
            </DropdownMenu.Item>
            <DropdownMenu.Item onclick={() => (keyboardShortcutsDialog.value = true)}>
              <CircleHelp aria-hidden="true" />
              Keyboard shortcuts
            </DropdownMenu.Item>
            <DropdownMenu.Item>
              {#snippet child({ props })}
                <a href={resolve('/about')} {...props}>
                  <Info aria-hidden="true" />
                  About
                </a>
              {/snippet}
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item variant="destructive" onclick={() => void closeActiveBook()}>
              <LogOut aria-hidden="true" />
              Close book
            </DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
</Sidebar.Root>
