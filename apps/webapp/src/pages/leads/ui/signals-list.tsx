import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { formatDateTime, formatRatio } from '#/pages/leads/lib/format'
import type { Signal } from '#/pages/leads/lib/types'
import { ProvenanceLink } from './provenance-link'

interface SignalsListProps {
	signals: readonly Signal[]
}

export function SignalsList({ signals }: SignalsListProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle>Signals ({signals.length})</CardTitle>
			</CardHeader>
			<CardContent>
				{signals.length === 0 ? (
					<p className="text-sm text-muted-foreground">No signal observed.</p>
				) : (
					<ul className="flex flex-col divide-y">
						{signals.map((signal) => (
							<li key={signal.id} className="flex flex-col gap-1 py-3">
								<div className="flex flex-wrap items-baseline gap-x-3">
									<span className="font-medium">{signal.kind}</span>
									{signal.value ? (
										<span className="text-sm">{signal.value}</span>
									) : null}
								</div>
								<div className="flex flex-wrap gap-x-4 text-xs text-muted-foreground">
									<span>Source: {signal.source}</span>
									<span>Strength: {formatRatio(signal.strength)}</span>
									<span>Observed: {formatDateTime(signal.observed_at)}</span>
								</div>
								<ProvenanceLink provenance={signal.provenance} />
							</li>
						))}
					</ul>
				)}
			</CardContent>
		</Card>
	)
}
