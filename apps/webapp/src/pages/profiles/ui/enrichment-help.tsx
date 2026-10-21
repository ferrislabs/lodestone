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
				L'enrichissement complète chaque lead du profil, quel que soit l'agent
				qui l'a trouvé. Il ne crée jamais de lead : un résultat sur un domaine
				sans lead dans ce profil est ignoré.
			</p>
			<p>
				La qualification attend que chaque étape activée ait tourné pour le
				lead, au plus {ENRICHMENT_WAIT_MINUTES} minutes après sa création.
			</p>
			<p className="text-xs text-muted-foreground">
				Un lead trouvé seulement par un certificat n'est jamais envoyé aux
				modèles : il attend une preuve d'un autre type, par exemple un produit
				détecté par la sonde HTTP ou une offre d'emploi.
			</p>
		</HelpPanel>
	)
}
