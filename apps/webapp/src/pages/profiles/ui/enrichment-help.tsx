import { ENRICHMENT_WAIT_MINUTES } from '#/pages/profiles/lib/enrichment'
import { HelpPanel } from '#/pages/profiles/ui/help-panel'

interface EnrichmentHelpProps {
	collapsed: boolean
	onToggle: () => void
}

export function EnrichmentHelp({ collapsed, onToggle }: EnrichmentHelpProps) {
	return (
		<HelpPanel title="Enrichissement" collapsed={collapsed} onToggle={onToggle}>
			<p>
				Chaque lead avec un domaine reçoit la même inspection de son écosystème,
				quel que soit le profil. Ce qu'elle trouve est partagé par les leads du
				même domaine et lu par vos agents de qualification.
			</p>
			<p>
				La qualification attend la fin de l'inspection, au plus{' '}
				{ENRICHMENT_WAIT_MINUTES} minutes après la création du lead.
			</p>
			<p className="text-xs text-muted-foreground">
				Un site hébergé sur une plateforme (github.io, vercel.app…) n'est pas lu
				: aucune page n'y est devinée.
			</p>
		</HelpPanel>
	)
}
