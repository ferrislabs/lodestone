import { Button } from '#/components/ui/button'
import { formatNumber } from '#/lib/format'
import { STRENGTH_ROWS } from '#/pages/profiles/lib/scoring-explain'
import { HelpPanel } from '#/pages/profiles/ui/help-panel'

interface ScoringExplainerProps {
	collapsed: boolean
	thresholdSummary: string | null
	example: string | null
	onToggle: () => void
	onNavigate: () => void
}

export function ScoringExplainer({
	collapsed,
	thresholdSummary,
	example,
	onToggle,
	onNavigate,
}: ScoringExplainerProps) {
	return (
		<HelpPanel
			title="Fonctionnement du scoring"
			collapsed={collapsed}
			onToggle={onToggle}
		>
			<ul className="flex list-disc flex-col gap-1 pl-4">
				<li>Un facteur cible un type de signal, éventuellement une valeur.</li>
				<li>
					Contribution = poids × force du signal le plus fort (0 à 1, définie
					par la source). Sans signal, 0.
				</li>
				<li>
					« Types de signaux distincts » : poids × nombre de types de signaux
					différents du lead.
				</li>
				<li>Score = somme des contributions. Score max = somme des poids.</li>
			</ul>
			<p>
				Le seuil de qualification (onglet Réglages) est comparé à score / score
				max. Au-delà, le LLM rédige une évaluation et un brouillon.
			</p>
			{thresholdSummary !== null && (
				<div className="flex flex-col items-start gap-2 font-medium">
					<p>{thresholdSummary}</p>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={onNavigate}
					>
						Modifier le seuil
					</Button>
				</div>
			)}
			{example !== null && (
				<p data-testid="scoring-example" className="text-xs">
					{example}
				</p>
			)}
			<div className="flex flex-col gap-1 text-xs">
				<p className="font-medium text-sm">Force du signal selon l'origine</p>
				<dl className="flex flex-col gap-1">
					{STRENGTH_ROWS.map((row) => (
						<div
							key={`${row.origin}-${row.strength}`}
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
		</HelpPanel>
	)
}
