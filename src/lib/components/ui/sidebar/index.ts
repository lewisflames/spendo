import { useSidebar } from './context.svelte.js';
import Content from './sidebar-content.svelte';
import Footer from './sidebar-footer.svelte';
import Group from './sidebar-group.svelte';
import GroupContent from './sidebar-group-content.svelte';
import GroupLabel from './sidebar-group-label.svelte';
import Inset from './sidebar-inset.svelte';
import Menu from './sidebar-menu.svelte';
import MenuBadge from './sidebar-menu-badge.svelte';
import MenuButton from './sidebar-menu-button.svelte';
import MenuItem from './sidebar-menu-item.svelte';
import Provider from './sidebar-provider.svelte';
import Root from './sidebar.svelte';
import Trigger from './sidebar-trigger.svelte';

export {
  Content,
  Footer,
  Group,
  GroupContent,
  GroupLabel,
  Inset,
  Menu,
  MenuBadge,
  MenuButton,
  MenuItem,
  Provider,
  Root,
  Trigger,
  //
  Root as Sidebar,
  Content as SidebarContent,
  Footer as SidebarFooter,
  Group as SidebarGroup,
  GroupContent as SidebarGroupContent,
  GroupLabel as SidebarGroupLabel,
  Inset as SidebarInset,
  Menu as SidebarMenu,
  MenuBadge as SidebarMenuBadge,
  MenuButton as SidebarMenuButton,
  MenuItem as SidebarMenuItem,
  Provider as SidebarProvider,
  Trigger as SidebarTrigger,
  useSidebar
};
