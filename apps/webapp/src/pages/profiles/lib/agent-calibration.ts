import { isJsonObject, type JsonValue } from '#/lib/schema-form'
import {
	calibrationStatusLabel,
	formatBrier,
} from '#/pages/quality/lib/calibration'
import type { Calibration } from '#/pages/quality/lib/types'

export type CalibrationKind = 'scoring' | 'qualification'

export interface AgentCalibrationState {
	status: string
	n: number
	required: number
	brierBefore: number | null
	brierAfter: number | null
}

export type CalibrationStates = Readonly<Record<string, AgentCalibrationState>>

export const NO_CALIBRATION_STATES: CalibrationStates = {}

export function calibrationKey(kind: CalibrationKind, agentId: string): string {
	return `${kind}:${agentId}`
}

export function agentLearnsCalibration(agent: JsonValue | undefined): boolean {
	return isJsonObject(agent) ? agent.learned_calibration !== false : true
}

export function calibrationStates(
	calibration: Calibration | null | undefined,
	profileId: string | null,
): CalibrationStates {
	if (calibration == null || profileId === null) return NO_CALIBRATION_STATES
	const profile = calibration.profiles.find(
		(item) => item.profile_id === profileId,
	)
	if (profile === undefined) return NO_CALIBRATION_STATES
	return Object.fromEntries(
		profile.agents.map((agent) => [
			`${agent.kind}:${agent.agent_id}`,
			{
				status: agent.status,
				n: agent.n,
				required: calibration.required,
				brierBefore: agent.brier_before ?? null,
				brierAfter: agent.brier_after ?? null,
			},
		]),
	)
}

export function calibrationStateLines(
	learned: boolean,
	state: AgentCalibrationState | undefined,
): string[] {
	if (!learned) return ['Désactivée']
	if (state === undefined) return []
	if (state.status === 'disabled') {
		return ["Activée à l'enregistrement de la révision"]
	}
	const lines = [calibrationStatusLabel(state, state.required)]
	if (state.brierBefore !== null) {
		lines.push(
			`Score de Brier avant ${formatBrier(state.brierBefore)}, après ${formatBrier(state.brierAfter)}`,
		)
	}
	return lines
}
