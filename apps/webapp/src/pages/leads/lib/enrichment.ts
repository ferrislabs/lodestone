import { formatDateTime } from '#/pages/leads/lib/format'
import type {
	Enrichment,
	EnrichmentStatus,
	EnrichmentStep,
	EnrichmentWaiting,
	EnrichmentWaitingReason,
	Signal,
} from '#/pages/leads/lib/types'

export type StepState = 'done' | 'pending' | 'disabled'

const STEP_LABELS: Readonly<Record<EnrichmentStep, string>> = {
	http_probe: 'Sonde HTTP',
	company_jobs: "Offres d'emploi de l'entreprise",
}

const STATE_LABELS: Readonly<Record<StepState, string>> = {
	done: 'Fait',
	pending: 'En attente',
	disabled: 'Désactivé',
}

const STATUS_LABELS: Readonly<Record<EnrichmentStatus, string>> = {
	disabled: 'Aucune étape activée',
	pending: 'En cours',
	complete: 'Terminé',
	timed_out: 'Délai dépassé',
}

const WAITING_LABELS: Readonly<Record<EnrichmentWaitingReason, string>> = {
	awaiting_enrichment: "En attente d'enrichissement",
	awaiting_enrichment_evidence: "En attente de preuve d'enrichissement",
}

export interface EnrichmentRow {
	step: EnrichmentStep
	label: string
	state: StepState
	stateLabel: string
	date: string | null
}

export function enrichmentRows(enrichment: Enrichment): EnrichmentRow[] {
	return enrichment.steps.map((item) => {
		const ranAt = item.ran_at ?? null
		const state: StepState = !item.enabled
			? 'disabled'
			: ranAt
				? 'done'
				: 'pending'
		return {
			step: item.step,
			label: STEP_LABELS[item.step],
			state,
			stateLabel: STATE_LABELS[state],
			date: state === 'done' && ranAt ? formatDateTime(ranAt) : null,
		}
	})
}

export function enrichmentStatusLabel(status: EnrichmentStatus): string {
	return STATUS_LABELS[status]
}

export interface WaitingBadge {
	reason: EnrichmentWaitingReason
	label: string
	detail: string | null
}

export function waitingBadge(
	enrichment: Enrichment | null | undefined,
): WaitingBadge | null {
	const reason = enrichment?.waiting_reason ?? null
	if (reason === null) return null
	const until = enrichment?.wait_until ?? null
	return {
		reason,
		label: WAITING_LABELS[reason],
		detail:
			reason === 'awaiting_enrichment' && until
				? `jusqu'au ${formatDateTime(until)}`
				: null,
	}
}

export function listWaitingLabel(
	waiting: EnrichmentWaiting | null | undefined,
): string | null {
	const reason = waiting?.waiting_reason ?? null
	return reason === null ? null : WAITING_LABELS[reason]
}

export function signalTitle(signal: Signal): string {
	const host = signal.value?.trim() ?? ''
	if (signal.kind === 'certificate_seen' && host !== '') {
		return `Certificat observé pour ${host}`
	}
	return signal.sentence
}
