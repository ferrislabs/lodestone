import { Link } from '@tanstack/react-router'
import type { LucideIcon } from 'lucide-react'

import {
	SidebarGroup,
	SidebarGroupLabel,
	SidebarMenu,
	SidebarMenuButton,
	SidebarMenuItem,
} from '#/components/ui/sidebar'

export interface NavItem {
	title: string
	to: string
	icon?: LucideIcon
	exact?: boolean
	disabled?: boolean
	badge?: string | number
}

interface NavMainProps {
	label?: string
	items: NavItem[]
}

export function NavMain({ label, items }: NavMainProps) {
	return (
		<SidebarGroup>
			{label ? (
				<SidebarGroupLabel className="text-[10px] font-semibold uppercase tracking-widest text-muted-foreground">
					{label}
				</SidebarGroupLabel>
			) : null}
			<SidebarMenu>
				{items.map((item) => (
					<SidebarMenuItem key={item.title}>
						{item.disabled ? (
							<SidebarMenuButton
								tooltip={`${item.title} · bientôt`}
								className="cursor-not-allowed rounded-lg opacity-50"
							>
								{item.icon ? <item.icon /> : null}
								<span>{item.title}</span>
								<span className="ml-auto rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground group-data-[collapsible=icon]:hidden">
									soon
								</span>
							</SidebarMenuButton>
						) : (
							<SidebarMenuButton asChild tooltip={item.title}>
								<Link
									to={item.to}
									activeOptions={item.exact ? { exact: true } : undefined}
									activeProps={{ 'data-active': 'true' }}
									className="rounded-lg data-[active=true]:bg-orange-50 data-[active=true]:font-semibold data-[active=true]:text-orange-600 data-[active=true]:[&_svg]:text-orange-600 dark:data-[active=true]:bg-orange-500/10 dark:data-[active=true]:text-orange-400"
								>
									{item.icon ? <item.icon /> : null}
									<span>{item.title}</span>
									{item.badge !== undefined ? (
										<span className="ml-auto text-xs font-medium text-muted-foreground group-data-[collapsible=icon]:hidden">
											{item.badge}
										</span>
									) : null}
								</Link>
							</SidebarMenuButton>
						)}
					</SidebarMenuItem>
				))}
			</SidebarMenu>
		</SidebarGroup>
	)
}
