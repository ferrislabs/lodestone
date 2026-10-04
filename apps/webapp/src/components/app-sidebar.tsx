import { Link } from '@tanstack/react-router'
import {
	BarChart3,
	Eye,
	FileText,
	Hammer,
	LayoutDashboard,
	Link2,
	Package,
	Receipt,
	Settings,
	ShieldCheck,
	Users,
} from 'lucide-react'
import type * as React from 'react'

import { type NavItem, NavMain } from '#/components/nav-main'
import {
	Sidebar,
	SidebarContent,
	SidebarFooter,
	SidebarHeader,
	SidebarMenu,
	SidebarMenuButton,
	SidebarMenuItem,
	SidebarRail,
} from '#/components/ui/sidebar'

const coreItems: NavItem[] = [
	{ title: 'Accueil', to: '/', icon: LayoutDashboard, exact: true },
	{ title: 'Clients', to: '/customers', icon: Users, badge: '3' },
	{ title: 'Devis', to: '/customers', icon: FileText, disabled: true },
	{ title: 'Factures', to: '/customers', icon: Receipt, disabled: true },
	{ title: 'Stock', to: '/customers', icon: Package, disabled: true },
]

const configItems: NavItem[] = [
	{ title: 'Paramètres', to: '/customers', icon: Settings, disabled: true },
	{ title: 'Intégrations', to: '/customers', icon: Link2, disabled: true },
	{ title: 'Rapports', to: '/customers', icon: BarChart3, disabled: true },
]

const securityItems: NavItem[] = [
	{ title: 'Audit', to: '/customers', icon: Eye, disabled: true },
	{
		title: 'Permissions',
		to: '/customers',
		icon: ShieldCheck,
		disabled: true,
	},
]

export function AppSidebar(props: React.ComponentProps<typeof Sidebar>) {
	return (
		<Sidebar collapsible="icon" {...props}>
			<SidebarHeader className="border-b">
				<SidebarMenu>
					<SidebarMenuItem>
						<SidebarMenuButton size="lg" asChild tooltip="Oxid">
							<Link to="/">
								<div className="flex aspect-square size-9 items-center justify-center rounded-xl bg-primary text-primary-foreground">
									<Hammer className="size-4" />
								</div>
								<div className="grid flex-1 text-left leading-tight">
									<span className="truncate text-base font-semibold">Oxid</span>
									<span className="truncate text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
										Console
									</span>
								</div>
							</Link>
						</SidebarMenuButton>
					</SidebarMenuItem>
				</SidebarMenu>
			</SidebarHeader>
			<SidebarContent>
				<NavMain label="Core" items={coreItems} />
				<NavMain label="Configuration" items={configItems} />
				<NavMain label="Sécurité" items={securityItems} />
			</SidebarContent>
			<SidebarFooter>
				<div className="flex items-center justify-start px-2 pb-1 text-[10px] font-medium group-data-[collapsible=icon]:hidden">
					<span className="rounded-md border bg-card px-1.5 py-0.5 text-orange-600">
						0.1.0
					</span>
				</div>
			</SidebarFooter>
			<SidebarRail />
		</Sidebar>
	)
}
