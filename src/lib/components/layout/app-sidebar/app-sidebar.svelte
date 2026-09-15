<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';

  import {
    ChevronsUpDown,
    CircleHelp,
    Database,
    FolderOpen,
    Info,
    LogOut,
    Plus,
    Save,
    Settings,
    SunMoon
  } from '@lucide/svelte';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import {
    closeActiveBook,
    createBookViaDialog,
    openBookViaDialog,
    saveBookAsViaDialog
  } from '$lib/features/book/book-actions';
  import { NAV_GROUPS, navHref, navItemActive } from '$lib/features/nav/nav-items';
  import { activeBookPath, dbSession } from '$lib/stores/db.svelte';
  import { navBadges, navRoots } from '$lib/stores/nav.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { keyboardShortcutsDialog } from '$lib/stores/ui.svelte';
  import { bookDisplayName, bookFolder } from '$lib/utils/book';
  import { plural } from '$lib/utils/format.svelte';

  const bookName = $derived(bookDisplayName(activeBookPath.value));
  const bookSubtitle = $derived(
    dbSession.value
      ? `${dbSession.value.summary.currency} · ${plural(dbSession.value.summary.totalTransactions, 'transaction')}`
      : null
  );

  // Resolve dynamic account-root hrefs, active state and badge counts once per
  // render pass; items without a target (roots still loading) are hidden.
  const groups = $derived(
    NAV_GROUPS.map((group) => ({
      label: group.label,
      items: group.items
        .map((item) => ({
          item,
          href: navHref(item, navRoots.value),
          active: navItemActive(item, page.url.pathname, navRoots.value),
          badge:
            item.badge === 'reconcile'
              ? navBadges.value.unclearedTransactions
              : item.badge === 'scheduled'
                ? navBadges.value.dueSchedules
                : 0
        }))
        .filter((entry) => entry.href !== null)
    })).filter((group) => group.items.length > 0)
  );
</script>

<Sidebar.Root class="top-(--header-height) h-[calc(100svh-var(--header-height))]!">
  <Sidebar.Content>
    <Sidebar.Group>
      <Sidebar.Menu>
        <Sidebar.MenuItem>
          <Sidebar.MenuButton
            class="bg-sidebar-primary text-sidebar-primary-foreground hover:bg-sidebar-primary/90 hover:text-sidebar-primary-foreground active:bg-sidebar-primary/90 active:text-sidebar-primary-foreground"
            tooltipContent="New transaction"
          >
            {#snippet child({ props })}
              <a href={resolve('/transactions/new')} {...props}>
                <Plus aria-hidden="true" />
                <span class="group-data-[collapsible=icon]:hidden">New transaction</span>
              </a>
            {/snippet}
          </Sidebar.MenuButton>
        </Sidebar.MenuItem>
      </Sidebar.Menu>
    </Sidebar.Group>

    {#each groups as group (group.label)}
      <Sidebar.Group>
        <Sidebar.GroupLabel>{group.label}</Sidebar.GroupLabel>
        <Sidebar.GroupContent>
          <Sidebar.Menu>
            {#each group.items as { item, href, active, badge } (item.id)}
              <Sidebar.MenuItem>
                <Sidebar.MenuButton
                  isActive={active}
                  tooltipContent={item.chord
                    ? `${item.label} · G ${item.chord.toUpperCase()}`
                    : item.label}
                >
                  {#snippet child({ props })}
                    <a
                      href={href === null ? undefined : resolve(href)}
                      {...props}
                      aria-current={active ? 'page' : undefined}
                    >
                      <item.icon aria-hidden="true" />
                      <span class="group-data-[collapsible=icon]:hidden">{item.label}</span>
                    </a>
                  {/snippet}
                </Sidebar.MenuButton>
                {#if badge > 0}
                  <Sidebar.MenuBadge
                    aria-label={`${badge} ${item.badge === 'reconcile' ? 'uncleared transactions' : 'scheduled transactions due'}`}
                  >
                    {badge}
                  </Sidebar.MenuBadge>
                {/if}
              </Sidebar.MenuItem>
            {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    {/each}
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
          <DropdownMenu.Content class="w-64" align="end" side="right">
            <DropdownMenu.Label>
              <div class="min-w-0">
                <span class="block truncate font-medium">{bookName}</span>
                {#if activeBookPath.value}
                  <span
                    class="block truncate text-xs text-muted-foreground"
                    title={activeBookPath.value}
                  >
                    {bookFolder(activeBookPath.value) ?? activeBookPath.value}
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
            <DropdownMenu.Item>
              {#snippet child({ props })}
                <a href={resolve('/settings')} {...props}>
                  <Settings aria-hidden="true" />
                  Settings
                </a>
              {/snippet}
            </DropdownMenu.Item>
            <DropdownMenu.Sub>
              <DropdownMenu.SubTrigger>
                <SunMoon aria-hidden="true" />
                Theme
              </DropdownMenu.SubTrigger>
              <DropdownMenu.SubContent>
                <DropdownMenu.RadioGroup
                  value={appSettings.value.theme}
                  onValueChange={(v) => {
                    if (v === 'light' || v === 'dark' || v === 'system') saveSettings({ theme: v });
                  }}
                >
                  <DropdownMenu.RadioItem value="light">Light</DropdownMenu.RadioItem>
                  <DropdownMenu.RadioItem value="dark">Dark</DropdownMenu.RadioItem>
                  <DropdownMenu.RadioItem value="system">System</DropdownMenu.RadioItem>
                </DropdownMenu.RadioGroup>
              </DropdownMenu.SubContent>
            </DropdownMenu.Sub>
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
