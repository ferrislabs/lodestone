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
				Un lead est évalué dès qu'il a une preuve, sans attendre l'inspection.
				Quand un module trouve du nouveau, le lead est réévalué quelques minutes
				plus tard, au plus trois fois par jour.
			</p>
			<p>
				Le bouton « Réévaluer » de la page d'un lead le réévalue à la demande.
			</p>
			<p className="text-xs text-muted-foreground">
				Un site hébergé sur une plateforme (github.io, vercel.app…) n'est pas lu
				: aucune page n'y est devinée.
			</p>
		</HelpPanel>
	)
}
