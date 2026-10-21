import type { Schemas } from '#/api/api.client'
import { formatDay, formatEuros, formatNumber } from '#/lib/format'

export type EcosystemModuleKey = Schemas.EcosystemModule
export type ModuleStatus = Schemas.EcosystemModuleStatus
export type EcosystemFinding = Schemas.EcosystemFindingDto

export type StatusTone =
	| 'done'
	| 'pending'
	| 'skipped'
	| 'failed'
	| 'off'
	| 'soon'

const STATUS_LABELS: Readonly<Record<ModuleStatus, string>> = {
	pending: 'En attente',
	done: 'Fait',
	not_applicable: 'Non applicable',
	failed: 'Échec',
}

const STATUS_TONES: Readonly<Record<ModuleStatus, StatusTone>> = {
	pending: 'pending',
	done: 'done',
	not_applicable: 'skipped',
	failed: 'failed',
}

export function moduleStatusLabel(status: ModuleStatus): string {
	return STATUS_LABELS[status]
}

export function statusTone(status: ModuleStatus): StatusTone {
	return STATUS_TONES[status]
}

export function platformNotice(
	platform: string | null | undefined,
): string | null {
	if (platform === null || platform === undefined || platform === '') {
		return null
	}
	return `Site hébergé sur une plateforme (${platform}) : les modules qui lisent le site ne s'appliquent pas.`
}

export const PROVIDER_NOT_READY = 'Fournisseur non prêt'

export function providerLink(
	provider: string | null | undefined,
	ready: boolean,
): string | null {
	if (ready || provider === null || provider === undefined || provider === '') {
		return null
	}
	return `/sources/providers/${provider}`
}

export function moduleNote(
	status: ModuleStatus,
	note: string | null | undefined,
): string | null {
	if (status !== 'failed' && status !== 'not_applicable') return null
	return note === null || note === undefined || note === '' ? null : note
}

export interface FindingRow {
	key: string
	label: string
	value: string
	sourceUrl: string
	highlighted: boolean
	date: string | null
	amount: string | null
	badge: string | null
	facts: string[]
	link: string | null
}

type Details = Record<string, unknown>

function text(details: Details, key: string): string | null {
	const value = details[key]
	return typeof value === 'string' && value.trim() !== '' ? value.trim() : null
}

function amountOf(details: Details): string | null {
	const value = details.amount_eur
	return typeof value === 'number' && Number.isFinite(value)
		? formatEuros(value)
		: null
}

function factsOf(finding: EcosystemFinding, details: Details): string[] {
	if (finding.module === 'legal_life') {
		const court = text(details, 'court')
		return court === null ? [] : [court]
	}
	if (finding.module === 'public_contracts') {
		const buyerSiret = text(details, 'buyer_siret')
		const buyer =
			text(details, 'buyer') ??
			(buyerSiret === null ? null : `Acheteur SIRET ${buyerSiret}`)
		const procedure = text(details, 'procedure')
		return [buyer, procedure].filter((fact): fact is string => fact !== null)
	}
	return []
}

function displayValue(finding: EcosystemFinding): string {
	return finding.kind === 'created_on'
		? formatDay(finding.value)
		: finding.value
}

export function findingRows(
	findings: readonly EcosystemFinding[],
): FindingRow[] {
	return findings.map((finding) => {
		const details: Details = finding.details ?? {}
		const date = text(details, 'date')
		const link = text(details, 'link')
		return {
			key: finding.key,
			label: finding.label,
			value: displayValue(finding),
			sourceUrl: finding.source_url,
			highlighted: details.highlighted === true,
			date: date === null ? null : formatDay(date),
			amount: amountOf(details),
			badge: details.collective === true ? 'Procédure collective' : null,
			facts: factsOf(finding, details),
			link: link !== null && /^https?:\/\//.test(link) ? link : null,
		}
	})
}

const MILLISECONDS_PER_SECOND = 1000

export function formatDuration(milliseconds: number): string {
	if (milliseconds < MILLISECONDS_PER_SECOND) return `${milliseconds} ms`
	return `${formatNumber(milliseconds / MILLISECONDS_PER_SECOND)} s`
}
