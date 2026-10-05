import { Outlet } from '@tanstack/react-router'
import { useAuth } from 'react-oidc-context'

import { AppSidebar } from '#/components/app-sidebar'
import {
	SidebarInset,
	SidebarProvider,
	SidebarTrigger,
} from '#/components/ui/sidebar'
import { UserMenu } from '#/components/user-menu'

export function AppShell() {
	const auth = useAuth()
	const profile = auth.user?.profile
	const displayName =
		profile?.name ||
		profile?.preferred_username ||
		profile?.email ||
		'Utilisateur'

	return (
		<SidebarProvider>
			<AppSidebar />
			<SidebarInset>
				<header className="sticky top-0 z-10 flex h-16 shrink-0 items-center gap-3 border-b bg-background/80 px-3 backdrop-blur md:px-6">
					<SidebarTrigger className="-ml-1" />
					<div className="ml-auto flex items-center gap-2">
						<UserMenu
							displayName={displayName}
							email={profile?.email ?? ''}
							onSignOut={() => {
								void auth.signoutRedirect()
							}}
						/>
					</div>
				</header>
				<div className="flex flex-1 flex-col">
					<Outlet />
				</div>
			</SidebarInset>
		</SidebarProvider>
	)
}
