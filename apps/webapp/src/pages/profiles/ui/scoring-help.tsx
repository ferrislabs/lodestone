import { SCORING_CONTRACT_EXAMPLE } from '#/pages/profiles/lib/scoring-agents'
import { HelpPanel } from '#/pages/profiles/ui/help-panel'

interface ScoringHelpProps {
	collapsed: boolean
	onToggle: () => void
}

const FLOW: readonly string[] = [
	'Sourcing : les agents trouvent des leads.',
	'Agents de qualification : chacun examine un angle et rapporte ses réponses, avec citations.',
	'Indicateurs : avis des agents × solidité des preuves, calculés par le système, donnés aux agents de scoring à titre d’information.',
	'Agents de scoring : chacun décide d’un score de 0 à 100 et d’un verdict.',
	'Résultat : moyenne pondérée des scores, verdict le plus probable ; une exclusion décidée par un agent de qualification écarte toujours le lead.',
	'Agrégateur : rédige la synthèse et le brouillon.',
]

export function ScoringHelp({ collapsed, onToggle }: ScoringHelpProps) {
	return (
		<HelpPanel title="Scoring" collapsed={collapsed} onToggle={onToggle}>
			<ol className="flex list-decimal flex-col gap-1.5 pl-4 text-xs">
				{FLOW.map((step) => (
					<li key={step}>{step}</li>
				))}
			</ol>
			<p>
				Un agent de scoring décrit ce qu'il évalue, choisit un fournisseur LLM,
				un prompt système et des critères à choix. Plusieurs agents peuvent
				coexister : chacun pèse de 1 à 10 dans le score et le verdict du lead.
				Sans agent de scoring actif, le verdict reste « en attente du scoring ».
			</p>
			<div className="flex flex-col gap-1">
				<p className="font-medium">JSON contractuel</p>
				<p className="text-xs text-muted-foreground">
					Le modèle répond uniquement avec cette structure, imposée par
					l'application :
				</p>
				<pre
					data-testid="scoring-contract-example"
					className="overflow-x-auto rounded-md bg-muted p-2 font-mono text-[11px]"
				>
					{SCORING_CONTRACT_EXAMPLE}
				</pre>
			</div>
		</HelpPanel>
	)
}
