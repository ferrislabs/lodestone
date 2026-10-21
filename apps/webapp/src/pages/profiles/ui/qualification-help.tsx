import { CONTRACT_EXAMPLE } from '#/pages/profiles/lib/qualification'
import { HelpPanel } from '#/pages/profiles/ui/help-panel'

interface QualificationHelpProps {
	collapsed: boolean
	onToggle: () => void
}

const FLOW: readonly string[] = [
	'Sourcing : les agents trouvent des leads.',
	"Agents de qualification : chacun mesure ses axes d'analyse sur chaque lead et en rédige un court résumé.",
	'Confiance calculée : somme pondérée des scores des réponses, jamais décidée par un modèle.',
	'Agrégateur : rédige la synthèse finale et le brouillon à partir des rapports de tous les agents.',
	'Bilan : verdict fiable, à vérifier ou écarté selon les seuils.',
]

export function QualificationHelp({
	collapsed,
	onToggle,
}: QualificationHelpProps) {
	return (
		<HelpPanel title="Qualification" collapsed={collapsed} onToggle={onToggle}>
			<ol className="flex list-decimal flex-col gap-1.5 pl-4 text-xs">
				{FLOW.map((step) => (
					<li key={step}>{step}</li>
				))}
			</ol>
			<p>
				Un agent de qualification décrit ce qu'il évalue, choisit un fournisseur
				LLM, un prompt système et des axes d'analyse à choix. Chaque option vaut
				un score de 0 à 1.
			</p>
			<div className="flex flex-col gap-1">
				<p className="font-medium">JSON contractuel</p>
				<p className="text-xs text-muted-foreground">
					Le modèle doit répondre uniquement avec :
				</p>
				<pre
					data-testid="contract-example"
					className="overflow-x-auto rounded-md bg-muted p-2 font-mono text-[11px]"
				>
					{CONTRACT_EXAMPLE}
				</pre>
			</div>
		</HelpPanel>
	)
}
