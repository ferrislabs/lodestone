import { Button } from '#/components/ui/button'
import { STRENGTH_ROWS } from '#/pages/profiles/lib/scoring-explain'

interface ScoringExplainerProps {
	factorCount: number
	maxScore: number
	thresholdSummary: string | null
	example: string | null
	onNavigate: () => void
}

export function ScoringExplainer({
	factorCount,
	maxScore,
	thresholdSummary,
	example,
	onNavigate,
}: ScoringExplainerProps) {
	return (
		<details
			open
			aria-label="How scoring works"
			className="rounded-xl border bg-card p-4 text-sm shadow-sm"
		>
			<summary className="cursor-pointer font-semibold">
				How scoring works
			</summary>
			<div className="mt-3 flex flex-col gap-3">
				<ul className="flex list-disc flex-col gap-1 pl-5">
					<li>
						Each factor looks for one signal kind, optionally one value. Its
						contribution is its weight times the strength of the strongest
						matching signal on the lead (0 to 1, set by the source). No matching
						signal contributes 0.
					</li>
					<li>
						"Distinct signal kinds" contributes its weight times the number of
						different signal kinds the lead has, which rewards leads seen by
						several agents.
					</li>
					<li>
						A lead's score is the sum of the contributions, in points. The max
						score is the sum of the weights, with "Distinct signal kinds"
						counted as its weight times the number of distinct kinds among the
						other factors.
					</li>
				</ul>
				<p className="font-medium">
					{`${factorCount} ${factorCount === 1 ? 'factor' : 'factors'} · max score ${maxScore} ${maxScore === 1 ? 'point' : 'points'}`}
				</p>
				<div className="flex flex-col gap-1">
					<p>
						The qualification threshold (Settings tab, in %) is compared with
						score / max score. Above it, the LLM writes an assessment and a
						draft message.
					</p>
					{thresholdSummary !== null && (
						<p className="flex flex-wrap items-center gap-2 font-medium">
							{thresholdSummary}
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={onNavigate}
							>
								Edit threshold
							</Button>
						</p>
					)}
				</div>
				{example !== null && <p data-testid="scoring-example">{example}</p>}
				<table className="w-full text-left text-xs">
					<caption className="pb-1 text-left font-medium text-sm">
						Signal strength by origin
					</caption>
					<thead>
						<tr>
							<th className="pr-2 font-medium">Origin</th>
							<th className="pr-2 font-medium">Signal</th>
							<th className="font-medium">Strength</th>
						</tr>
					</thead>
					<tbody>
						{STRENGTH_ROWS.map((row) => (
							<tr key={`${row.origin}-${row.strength}`}>
								<td className="pr-2">{row.origin}</td>
								<td className="pr-2">{row.signal}</td>
								<td>{row.strength}</td>
							</tr>
						))}
					</tbody>
				</table>
			</div>
		</details>
	)
}
