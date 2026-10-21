import { ArrowLeft } from 'lucide-react'
import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import { failureOfKind } from '#/pages/leads/lib/agent-failure'
import {
	canReevaluate,
	changedSince,
	evaluatedAtText,
	evaluationLabel,
	inspectionProgress,
} from '#/pages/leads/lib/evaluation'
import type { LeadDetailData, LeadStatus } from '#/pages/leads/lib/types'
import { AgentStopsNotice } from './agent-stops-notice'
import { AssessmentsList } from './assessments-list'
import { CompanyCard } from './company-card'
import { ContactsList } from './contacts-list'
import { ExclusionNotice } from './exclusion-notice'
import { LeadEcosystem } from './lead-ecosystem'
import { LeadEvaluation } from './lead-evaluation'
import { LeadFacts } from './lead-facts'
import { LeadHeader } from './lead-header'
import { LeadSummary } from './lead-summary'
import { QualificationResults } from './qualification-results'
import { ScoringResults } from './scoring-results'
import { SignalsList } from './signals-list'

interface LeadDetailProps {
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	detail: LeadDetailData | null
	nextStatuses: readonly LeadStatus[]
	onBack: () => void
	onOpenLead: (leadId: string) => void
	onChangeStatus: (status: LeadStatus) => void
	rejectionDialog: ReactNode
	pipeline?: ReactNode
	chronology?: ReactNode
	onOpenDraft: (assessmentId: string) => void
	onRetrySummary: () => void
	retryingSummary: boolean
	retrySummaryError: string | null
	onRetryAgents: () => void
	retryingAgents: boolean
	retryAgentsError: string | null
	onRefreshCompany: () => void
	refreshingCompany: boolean
	refreshCompanyError: string | null
	onReevaluate: () => void
	reevaluating: boolean
	reevaluateError: string | null
	changing: boolean
	changeError: string | null
}

export function LeadDetail({
	state,
	errorMessage,
	detail,
	nextStatuses,
	onBack,
	onOpenLead,
	onChangeStatus,
	rejectionDialog,
	pipeline = null,
	chronology = null,
	onOpenDraft,
	onRetrySummary,
	retryingSummary,
	retrySummaryError,
	onRetryAgents,
	retryingAgents,
	retryAgentsError,
	onRefreshCompany,
	refreshingCompany,
	refreshCompanyError,
	onReevaluate,
	reevaluating,
	reevaluateError,
	changing,
	changeError,
}: LeadDetailProps) {
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					<ArrowLeft />
					Retour aux leads
				</Button>
			</div>
			{state === 'loading' ? (
				<div className="flex flex-col gap-3" aria-busy="true">
					<Skeleton className="h-8 w-64" />
					<Skeleton className="h-24 w-full" />
					<Skeleton className="h-48 w-full" />
				</div>
			) : null}
			{state === 'error' ? (
				<p role="alert" className="text-destructive">
					Impossible de charger ce lead : {errorMessage}
				</p>
			) : null}
			{state === 'ready' && detail ? (
				<>
					<LeadHeader
						lead={detail.lead}
						company={detail.company ?? null}
						onOpenLead={onOpenLead}
						nextStatuses={nextStatuses}
						onChangeStatus={onChangeStatus}
						changing={changing}
						changeError={changeError}
					/>
					{pipeline}
					<LeadEvaluation
						stage={detail.lead.evaluation?.stage ?? null}
						label={evaluationLabel(detail.lead.evaluation)}
						evaluatedAt={evaluatedAtText(detail.lead.evaluation)}
						changed={changedSince(detail.lead.evaluation)}
						canReevaluate={canReevaluate(detail.lead.evaluation)}
						reevaluating={reevaluating}
						reevaluateError={reevaluateError}
						onReevaluate={onReevaluate}
					/>
					<ExclusionNotice exclusions={detail.lead.exclusions} />
					<CompanyCard
						company={detail.company ?? null}
						refreshing={refreshingCompany}
						refreshError={refreshCompanyError}
						onRefresh={onRefreshCompany}
					/>
					{detail.facts ? <LeadFacts facts={detail.facts} /> : null}
					{detail.ecosystem ? (
						<LeadEcosystem
							ecosystem={detail.ecosystem}
							progress={inspectionProgress(detail.lead.evaluation)}
						/>
					) : null}
					<AgentStopsNotice stops={detail.agent_stops ?? []} />
					<QualificationResults
						results={detail.qualification_results}
						failure={failureOfKind(detail.agent_failures, 'qualification')}
						onRetry={onRetryAgents}
						retrying={retryingAgents}
						retryError={retryAgentsError}
					/>
					<ScoringResults
						results={detail.scoring_results}
						lead={detail.lead}
						failure={failureOfKind(detail.agent_failures, 'scoring')}
						onRetry={onRetryAgents}
						retrying={retryingAgents}
						retryError={retryAgentsError}
					/>
					<LeadSummary
						assessment={detail.assessments[0] ?? null}
						summaryState={detail.summary_state ?? null}
						onOpenDraft={onOpenDraft}
						onRetry={onRetrySummary}
						retrying={retryingSummary}
						retryError={retrySummaryError}
					/>
					<SignalsList signals={detail.signals} />
					<ContactsList contacts={detail.contacts} />
					{chronology}
					{detail.assessments.length > 1 ? (
						<AssessmentsList assessments={detail.assessments.slice(1)} />
					) : null}
					{rejectionDialog}
				</>
			) : null}
		</div>
	)
}
