<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';

  import {
    ArrowLeftRight,
    Settings,
    Info,
    House,
    Receipt,
    PiggyBank,
    Briefcase,
    CircleHelp,
    FolderTree
  } from '@lucide/svelte';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { keyboardShortcutsDialog } from '$lib/stores/keyboard-shortcuts.svelte';
</script>

<Sidebar.Root class="top-(--header-height) h-[calc(100svh-var(--header-height))]!">
  <Sidebar.Content>
    <Sidebar.Group>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton isActive={page.url.pathname === '/'} tooltipContent="Overview">
              {#snippet child({ props })}
                <a href={resolve('/')} {...props}>
                  <House aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Overview</span>
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
              isActive={page.url.pathname === '/transactions'}
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
              isActive={page.url.pathname === '/pnl'}
              tooltipContent="Profit & Loss"
            >
              {#snippet child({ props })}
                <a href={resolve('/pnl')} {...props}>
                  <Receipt aria-hidden="true" />
                  <span class="group-data-[collapsible=icon]:hidden">Profit &amp; Loss</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/budgets'}
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
        <Sidebar.MenuButton isActive={page.url.pathname === '/about'} tooltipContent="About">
          {#snippet child({ props })}
            <a href={resolve('/about')} {...props}>
              <Info aria-hidden="true" />
              <span class="group-data-[collapsible=icon]:hidden">About</span>
            </a>
          {/snippet}
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton
          tooltipContent="Keyboard shortcuts"
          onclick={() => (keyboardShortcutsDialog.value = true)}
        >
          <CircleHelp aria-hidden="true" />
          <span class="group-data-[collapsible=icon]:hidden">Shortcuts</span>
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton isActive={page.url.pathname === '/settings'} tooltipContent="Settings">
          {#snippet child({ props })}
            <a href={resolve('/settings')} {...props}>
              <Settings aria-hidden="true" />
              <span class="group-data-[collapsible=icon]:hidden">Settings</span>
            </a>
          {/snippet}
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
</Sidebar.Root>
