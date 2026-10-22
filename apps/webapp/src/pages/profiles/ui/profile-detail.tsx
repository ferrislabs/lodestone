import type { ReactNode } from 'react'

import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { ProfileTab, TabItem } from '#/pages/profiles/lib/tabs'
import type { ProfileDetail as ProfileDetailData } from '#/pages/profiles/lib/types'
import { TabBar } from '#/pages/profiles/ui/tab-bar'

interface ProfileDetailProps {
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	profile: ProfileDetailData | null
	activation: ReactNode
	tab: ProfileTab
	tabs: readonly TabItem[]
	exporting: boolean
	exportError: string | null
	definition: ReactNode
	history: ReactNode
	trials: ReactNode
	onTabChange: (tab: ProfileTab) => void
	onExport: () => void
	onBack: () => void
}

export function ProfileDetail({
	state,
	errorMessage,
	profile,
	activation,
	tab,
	tabs,
	exporting,
	exportError,
	definition,
	history,
	trials,
	onTabChange,
	onExport,
	onBack,
}: ProfileDetailProps) {
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					Retour aux profils
				</Button>
			</div>
			{state === 'loading' ? <Skeleton className="h-24 w-full" /> : null}
			{state === 'error' ? (
				<p role="alert" className="text-destructive">
					Impossible de charger le profil : {errorMessage}
				</p>
			) : null}
			{state === 'ready' && profile ? (
				<>
					<div className="flex flex-wrap items-start justify-between gap-3">
						<div className="flex flex-col gap-1">
							<h1 className="text-2xl font-bold tracking-tight">
								{profile.name}
							</h1>
							<div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
								<Badge variant={profile.active ? 'default' : 'secondary'}>
									{profile.active ? 'Actif' : 'Inactif'}
								</Badge>
								<span>Révision {profile.revision}</span>
								<span>Créé le {formatDateTime(profile.created_at)}</span>
								<span>Mis à jour le {formatDateTime(profile.updated_at)}</span>
							</div>
						</div>
						<div className="flex flex-wrap items-center gap-2">
							{activation}
							<Button
								size="sm"
								variant="outline"
								disabled={exporting}
								onClick={onExport}
							>
								Exporter en JSON
							</Button>
						</div>
					</div>
					{exportError ? (
						<p role="alert" className="text-sm text-destructive">
							{exportError}
						</p>
					) : null}
					<TabBar tabs={tabs} active={tab} onChange={onTabChange} />
					{tab === 'history' ? history : tab === 'trials' ? trials : definition}
				</>
			) : null}
		</div>
	)
}
