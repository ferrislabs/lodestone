import type {
	AgentCalibration,
	ProfileCalibration,
	ReliabilityBin,
} from '#/pages/quality/lib/types'

export const CURVE_SIZE = 100

const BRIER = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 3 })

export interface CurvePoint {
	x: number
	y: number
	n: number
}

const KIND_LABELS: Readonly<Record<string, string>> = {
	scoring: 'Scoring',
	qualification: 'Qualification',
}

export function calibrationKindLabel(kind: AgentCalibration['kind']): string {
	return KIND_LABELS[kind] ?? kind
}

export function calibrationStatusLabel(
	agent: Pick<AgentCalibration, 'status' | 'n'>,
	required: number,
): string {
	switch (agent.status) {
		case 'active':
			return 'Active'
		case 'disabled':
			return 'Désactivée'
		default:
			return `Pas assez de décisions (${agent.n}/${required})`
	}
}

export function formatBrier(value: number | null | undefined): string {
	return value == null ? '—' : BRIER.format(value)
}

function round(value: number): number {
	return Math.round(value * 10) / 10
}

export function curvePoints(bins: readonly ReliabilityBin[]): CurvePoint[] {
	return [...bins]
		.sort((a, b) => a.predicted - b.predicted)
		.map((bin) => ({
			x: round(Math.min(1, Math.max(0, bin.predicted)) * CURVE_SIZE),
			y: round(
				CURVE_SIZE - Math.min(1, Math.max(0, bin.observed)) * CURVE_SIZE,
			),
			n: bin.n,
		}))
}

export function curveLine(points: readonly CurvePoint[]): string {
	return points.map((point) => `${point.x},${point.y}`).join(' ')
}

export function visibleProfiles(
	profiles: readonly ProfileCalibration[],
	profileId: string,
): ProfileCalibration[] {
	return profileId === ''
		? [...profiles]
		: profiles.filter((profile) => profile.profile_id === profileId)
}

export function recalibrationNotice(queued: boolean): string {
	return queued
		? 'Recalcul programmé : il démarre dans quelques instants, rechargez la page ensuite.'
		: 'Un recalcul est déjà en attente.'
}
