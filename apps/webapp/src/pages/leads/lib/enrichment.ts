import {
	type FindingRow,
	findingRows,
	moduleStatusLabel,
	type StatusTone,
	statusTone,
} from '#/lib/ecosystem'
import { formatDateTime } from '#/pages/leads/lib/format'
import type {
	Ecosystem,
	Enrichment,
	EnrichmentStatus,
	EnrichmentWaiting,
	EnrichmentWaitingReason,
	Signal,
} from '#/pages/leads/lib/types'

const STATUS_LABELS: Readonly<Record<EnrichmentStatus, string>> = {
	disabled: 'Inspection désactivée',
	pending: 'En cours',
	complete: 'Terminé',
	timed_out: 'Délai dépassé',
}

const WAITING_LABELS: Readonly<Record<EnrichmentWaitingReason, string>> = {
	awaiting_enrichment: "En attente d'enrichissement",
	awaiting_enrichment_evidence: "En attente de preuve d'enrichissement",
}

export interface ModuleSection {
	module: string
	label: string
	statusLabel: string
	tone: StatusTone
	lastRun: string | null
	error: string | null
	rows: FindingRow[]
}

export function moduleSections(ecosystem: Ecosystem): ModuleSection[] {
	return ecosystem.modules.map((report) => ({
		module: report.module,
		label: report.label,
		statusLabel: moduleStatusLabel(report.status),
		tone: statusTone(report.status),
		lastRun: report.last_run ? formatDateTime(report.last_run) : null,
		error: report.status === 'failed' ? (report.error ?? null) : null,
		rows: findingRows(report.findings),
	}))
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
