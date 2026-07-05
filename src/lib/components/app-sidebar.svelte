<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';

  import {
    LayoutDashboard,
    ArrowLeftRight,
    Settings,
    Info,
    FileText,
    Wallet,
    Database,
    TrendingUp
  } from '@lucide/svelte';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { dbSession } from '$lib/stores/db.svelte';
</script>

<Sidebar.Root collapsible="icon">
  <Sidebar.Header>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton
          size="lg"
          class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
        >
          {#snippet child({ props })}
            <a href={resolve('/')} class="flex flex-row items-center gap-2" {...props}>
              <div
                class="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg"
              >
                <Wallet aria-hidden="true" class="size-4" />
              </div>
              <span class="truncate font-medium">Spendo</span>
            </a>
          {/snippet}
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Header>
  <Sidebar.Content>
    <Sidebar.Group>
      <Sidebar.GroupLabel>Navigation</Sidebar.GroupLabel>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/dashboard'}
              aria-disabled={!dbSession.value || undefined}
              tooltipContent="Dashboard"
            >
              {#snippet child({ props })}
                <a href={dbSession.value ? resolve('/dashboard') : undefined} {...props}>
                  <LayoutDashboard aria-hidden="true" />
                  <span>Dashboard</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/accounts' ||
                page.url.pathname.startsWith('/accounts/')}
              aria-disabled={!dbSession.value || undefined}
              tooltipContent="Accounts"
            >
              {#snippet child({ props })}
                <a href={dbSession.value ? resolve('/accounts') : undefined} {...props}>
                  <Database aria-hidden="true" />
                  <span>Accounts</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/transactions'}
              aria-disabled={!dbSession.value || undefined}
              tooltipContent="Transactions"
            >
              {#snippet child({ props })}
                <a href={dbSession.value ? resolve('/transactions') : undefined} {...props}>
                  <ArrowLeftRight aria-hidden="true" />
                  <span>Transactions</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={page.url.pathname === '/prices'}
              aria-disabled={!dbSession.value || undefined}
              tooltipContent="Prices"
            >
              {#snippet child({ props })}
                <a href={dbSession.value ? resolve('/prices') : undefined} {...props}>
                  <TrendingUp aria-hidden="true" />
                  <span>Prices</span>
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
        <Sidebar.MenuButton isActive={page.url.pathname === '/license'} tooltipContent="License">
          {#snippet child({ props })}
            <a href={resolve('/license')} {...props}>
              <FileText aria-hidden="true" />
              <span>License</span>
            </a>
          {/snippet}
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton isActive={page.url.pathname === '/about'} tooltipContent="About">
          {#snippet child({ props })}
            <a href={resolve('/about')} {...props}>
              <Info aria-hidden="true" />
              <span>About</span>
            </a>
          {/snippet}
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton isActive={page.url.pathname === '/settings'} tooltipContent="Settings">
          {#snippet child({ props })}
            <a href={resolve('/settings')} {...props}>
              <Settings aria-hidden="true" />
              <span>Settings</span>
            </a>
          {/snippet}
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
  <Sidebar.Rail />
</Sidebar.Root>
