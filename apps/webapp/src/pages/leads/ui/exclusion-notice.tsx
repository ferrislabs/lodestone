import { exclusionTitle } from '#/pages/leads/lib/confidence'
import type { Exclusion } from '#/pages/leads/lib/types'
import { ProvenanceLink } from './provenance-link'

interface ExclusionNoticeProps {
	exclusions: readonly Exclusion[]
}

function EvidenceSource({ source }: { source: string }) {
	if (source === 'lead') {
		return <span>Faits calculés par le système</span>
	}
	return <ProvenanceLink provenance={source} />
}

export function ExclusionNotice({ exclusions }: ExclusionNoticeProps) {
	if (exclusions.length === 0) return null
	return (
		<section
			aria-label="Exclusion du lead"
			data-section="exclusion"
			className="flex flex-col gap-3 rounded-lg border-2 border-red-300 bg-red-50 p-4 text-red-900"
		>
			{exclusions.map((exclusion) => (
				<div
					key={`${exclusion.agent_id}-${exclusion.question_id}-${exclusion.option}`}
					data-exclusion={exclusion.question_id}
					className="flex flex-col gap-1"
				>
					<p className="text-base font-semibold">{exclusionTitle(exclusion)}</p>
					{exclusion.evidence ? (
						<div className="flex flex-col gap-0.5 border-l-2 border-red-300 pl-2 text-sm">
							<p className="italic" data-part="exclusion-quote">
								« {exclusion.evidence} »
							</p>
							{exclusion.evidence_source ? (
								<p className="text-xs text-red-800">
									Source : <EvidenceSource source={exclusion.evidence_source} />
								</p>
							) : null}
						</div>
					) : null}
				</div>
			))}
		</section>
	)
}
