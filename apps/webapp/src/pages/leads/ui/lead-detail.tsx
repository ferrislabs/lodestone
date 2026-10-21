import { ArrowLeft } from 'lucide-react'
import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import type { LeadDetailData, LeadStatus } from '#/pages/leads/lib/types'
import { AssessmentsList } from './assessments-list'
import { CompanyCard } from './company-card'
import { ContactsList } from './contacts-list'
import { LeadFacts } from './lead-facts'
import { LeadHeader } from './lead-header'
import { LeadSummary } from './lead-summary'
import { QualificationResults } from './qualification-results'
import { ScoreBreakdown } from './score-breakdown'
import { SignalsList } from './signals-list'

interface LeadDetailProps {
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	detail: LeadDetailData | null
	nextStatuses: readonly LeadStatus[]
	onBack: () => void
	onChangeStatus: (status: LeadStatus) => void
	rejectionDialog: ReactNode
	onOpenDraft: (assessmentId: string) => void
	onRetrySummary: () => void
	retryingSummary: boolean
	retrySummaryError: string | null
	onRefreshCompany: () => void
	refreshingCompany: boolean
	refreshCompanyError: string | null
	changing: boolean
	changeError: string | null
}

export function LeadDetail({
	state,
	errorMessage,
	detail,
	nextStatuses,
	onBack,
	onChangeStatus,
	rejectionDialog,
	onOpenDraft,
	onRetrySummary,
	retryingSummary,
	retrySummaryError,
	onRefreshCompany,
	refreshingCompany,
	refreshCompanyError,
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
						nextStatuses={nextStatuses}
						onChangeStatus={onChangeStatus}
						changing={changing}
						changeError={changeError}
					/>
					<CompanyCard
						company={detail.company ?? null}
						refreshing={refreshingCompany}
						refreshError={refreshCompanyError}
						onRefresh={onRefreshCompany}
					/>
					{detail.facts ? <LeadFacts facts={detail.facts} /> : null}
					<QualificationResults results={detail.qualification_results} />
					<LeadSummary
						assessment={detail.assessments[0] ?? null}
						summaryState={detail.summary_state ?? null}
						onOpenDraft={onOpenDraft}
						onRetry={onRetrySummary}
						retrying={retryingSummary}
						retryError={retrySummaryError}
					/>
					<ScoreBreakdown contributions={detail.score_breakdown} />
					<SignalsList signals={detail.signals} />
					<ContactsList contacts={detail.contacts} />
					{detail.assessments.length > 1 ? (
						<AssessmentsList assessments={detail.assessments.slice(1)} />
					) : null}
					{rejectionDialog}
				</>
			) : null}
		</div>
	)
}
