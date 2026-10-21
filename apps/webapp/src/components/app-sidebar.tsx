import { Link } from '@tanstack/react-router'
import { Cpu, FileText, Hammer, Layers, Users } from 'lucide-react'
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
import { usePendingDraftsCount } from '#/pages/drafts/feature/pending-drafts-count'

export function AppSidebar(props: React.ComponentProps<typeof Sidebar>) {
	const pending = usePendingDraftsCount()
	const coreItems: NavItem[] = [
		{ title: 'Leads', to: '/leads', icon: Users },
		{ title: 'Brouillons', to: '/drafts', icon: FileText, badge: pending },
	]
	const configItems: NavItem[] = [
		{ title: 'Profils', to: '/profiles', icon: Layers },
		{ title: 'Fournisseurs LLM', to: '/llm', icon: Cpu },
	]
	return (
		<Sidebar collapsible="icon" {...props}>
			<SidebarHeader className="border-b">
				<SidebarMenu>
					<SidebarMenuItem>
						<SidebarMenuButton size="lg" asChild tooltip="Lodestone">
							<Link to="/">
								<div className="flex aspect-square size-9 items-center justify-center rounded-xl bg-primary text-primary-foreground">
									<Hammer className="size-4" />
								</div>
								<div className="grid flex-1 text-left leading-tight">
									<span className="truncate text-base font-semibold">
										Lodestone
									</span>
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
				<NavMain items={coreItems} />
				<NavMain label="Configuration" items={configItems} />
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
