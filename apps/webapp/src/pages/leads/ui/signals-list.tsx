import { Badge } from '#/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { signalKindLabel, sourceLabel } from '#/lib/labels'
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
				<CardTitle>Signaux ({signals.length})</CardTitle>
			</CardHeader>
			<CardContent>
				{signals.length === 0 ? (
					<p className="text-sm text-muted-foreground">Aucun signal observé.</p>
				) : (
					<ul className="flex flex-col divide-y">
						{signals.map((signal) => (
							<li key={signal.id} className="flex flex-col gap-1 py-3">
								<div className="flex flex-wrap items-baseline gap-x-3">
									<span className="font-medium">
										{signalKindLabel(signal.kind)}
									</span>
									{signal.value ? (
										<span className="text-sm">{signal.value}</span>
									) : null}
									{signal.agent ? (
										<Badge variant="secondary">{signal.agent}</Badge>
									) : null}
								</div>
								<div className="flex flex-wrap gap-x-4 text-xs text-muted-foreground">
									<span>Source : {sourceLabel(signal.source)}</span>
									<span>Force : {formatRatio(signal.strength)}</span>
									<span>Observé : {formatDateTime(signal.observed_at)}</span>
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
