import { formatNumber } from '#/lib/format'
import { SCORING_CONTRACT_EXAMPLE } from '#/pages/profiles/lib/scoring-agents'
import { STRENGTH_ROWS } from '#/pages/profiles/lib/scoring-explain'
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
			<p>
				La force des signaux (0 à 100) ordonne la liste des leads. C'est la
				moyenne des trois signaux indépendants les plus forts, chacun avec la
				force donnée par sa source et diminué selon son âge. Elle ne décide pas
				du verdict et ne se règle pas dans le profil.
			</p>
			<div className="flex flex-col gap-1 text-xs">
				<p className="font-medium text-sm">Force du signal selon l'origine</p>
				<dl className="flex flex-col gap-1">
					{STRENGTH_ROWS.map((row) => (
						<div
							key={`${row.origin}-${row.signal}-${row.strength}`}
							className="flex justify-between gap-2"
						>
							<dt className="text-muted-foreground">
								{row.origin} · {row.signal}
							</dt>
							<dd className="font-mono">→ {formatNumber(row.strength)}</dd>
						</div>
					))}
				</dl>
			</div>
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
