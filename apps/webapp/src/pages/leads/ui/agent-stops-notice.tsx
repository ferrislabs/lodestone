import { stopReasonLabel } from '#/lib/agent-stop'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { AgentStop } from '#/pages/leads/lib/types'

export function AgentStopsNotice({ stops }: { stops: readonly AgentStop[] }) {
	if (stops.length === 0) return null
	return (
		<section
			data-section="agent-stops"
			className="flex flex-col gap-2 rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900"
		>
			<h2 className="font-medium">Travail interrompu</h2>
			<ul className="flex flex-col gap-1">
				{stops.map((stop) => (
					<li key={`${stop.kind}-${stop.agent_id}`} data-agent-stop={stop.kind}>
						Agent « {stop.agent_name} » : {stopReasonLabel(stop.reason)}
						<span className="text-amber-700">
							{' '}
							· {formatDateTime(stop.stopped_at)}
						</span>
					</li>
				))}
			</ul>
		</section>
	)
}
