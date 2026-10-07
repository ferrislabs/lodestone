import { formatDateTime } from '#/pages/leads/lib/format'
import type { AgentFailure } from '#/pages/leads/lib/types'

export function failureOfKind(
	failures: readonly AgentFailure[],
	kind: AgentFailure['kind'],
): AgentFailure | null {
	return failures.find((failure) => failure.kind === kind) ?? null
}

export function retryStatusLabel(failure: AgentFailure): string {
	if (failure.failed) return `En échec après ${failure.attempts} tentatives`
	return failure.next_attempt_at
		? `Nouvelle tentative : ${formatDateTime(failure.next_attempt_at)}`
		: 'Nouvelle tentative au prochain passage'
}

export function awaitingFailureLabel(failure: AgentFailure): string {
	if (failure.failed) {
		return `En attente : appel au modèle en échec après ${failure.attempts} tentatives`
	}
	return failure.next_attempt_at
		? `En attente : appel au modèle en échec, nouvelle tentative ${formatDateTime(failure.next_attempt_at)}`
		: 'En attente : appel au modèle en échec, nouvelle tentative au prochain passage'
}
