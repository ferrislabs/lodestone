import type { Schemas } from '#/api/api.client'
import { formatNumber } from '#/lib/format'

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

export interface FindingRow {
	key: string
	label: string
	value: string
	sourceUrl: string
	highlighted: boolean
}

export function findingRows(
	findings: readonly EcosystemFinding[],
): FindingRow[] {
	return findings.map((finding) => ({
		key: finding.key,
		label: finding.label,
		value: finding.value,
		sourceUrl: finding.source_url,
		highlighted: finding.details.highlighted === true,
	}))
}

const MILLISECONDS_PER_SECOND = 1000

export function formatDuration(milliseconds: number): string {
	if (milliseconds < MILLISECONDS_PER_SECOND) return `${milliseconds} ms`
	return `${formatNumber(milliseconds / MILLISECONDS_PER_SECOND)} s`
}
