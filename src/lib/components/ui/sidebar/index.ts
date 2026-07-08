import { useSidebar } from './context.svelte.js';
import Content from './sidebar-content.svelte';
import Footer from './sidebar-footer.svelte';
import Group from './sidebar-group.svelte';
import GroupContent from './sidebar-group-content.svelte';
import Inset from './sidebar-inset.svelte';
import Menu from './sidebar-menu.svelte';
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
  Inset,
  Menu,
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
  Inset as SidebarInset,
  Menu as SidebarMenu,
  MenuButton as SidebarMenuButton,
  MenuItem as SidebarMenuItem,
  Provider as SidebarProvider,
  Trigger as SidebarTrigger,
  useSidebar
};
