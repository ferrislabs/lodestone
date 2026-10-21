import { formatDateTime } from '#/pages/leads/lib/format'
import type { Evaluation, EvaluationStage } from '#/pages/leads/lib/types'

export const EVALUATION_POLL_MS = 15_000

const CLOCK = new Intl.DateTimeFormat('fr-FR', {
	hour: '2-digit',
	minute: '2-digit',
})

const MOVING_STAGES: readonly EvaluationStage[] = [
	'not_evaluated',
	'requalification_planned',
	'inspection_running',
]

export function formatClock(iso: string): string {
	return CLOCK.format(new Date(iso))
}

export function evaluationLabel(
	evaluation: Evaluation | null | undefined,
): string | null {
	if (!evaluation) return null
	switch (evaluation.stage) {
		case 'not_evaluated':
			return 'Non évalué'
		case 'certificate_only':
			return "Certificat seul — en attente d'une autre preuve"
		case 'requalification_planned': {
			const reason =
				evaluation.requalify_reason === 'manual'
					? 'demandée'
					: 'nouvelles informations'
			return evaluation.requalify_at
				? `Réévaluation prévue à ${formatClock(evaluation.requalify_at)} (${reason})`
				: `Réévaluation prévue (${reason})`
		}
		case 'inspection_running':
			return `Évalué — inspection en cours (${evaluation.modules_done}/${evaluation.modules_available} modules)`
		case 'inspection_complete':
			return 'Évalué — inspection complète'
		case 'evaluated':
			return 'Évalué'
	}
}

export function evaluatedAtText(
	evaluation: Evaluation | null | undefined,
): string | null {
	const at = evaluation?.evaluated_at ?? null
	return at ? `Dernière évaluation le ${formatDateTime(at)}` : null
}

export function inspectionProgress(
	evaluation: Evaluation | null | undefined,
): string | null {
	if (!evaluation || evaluation.modules_available === 0) return null
	return `${evaluation.modules_done}/${evaluation.modules_available} modules inspectés`
}

export interface ChangedModule {
	module: string
	label: string
	at: string | null
}

export function changedSince(
	evaluation: Evaluation | null | undefined,
): ChangedModule[] {
	return (evaluation?.changed_modules ?? []).map((state) => ({
		module: state.module,
		label: state.label,
		at: state.changed_at ? formatDateTime(state.changed_at) : null,
	}))
}

export function evaluationMoves(
	evaluation: Evaluation | null | undefined,
): boolean {
	return evaluation ? MOVING_STAGES.includes(evaluation.stage) : false
}

export function canReevaluate(
	evaluation: Evaluation | null | undefined,
): boolean {
	return (
		evaluation !== null &&
		evaluation !== undefined &&
		evaluation.stage !== 'not_evaluated' &&
		evaluation.stage !== 'certificate_only'
	)
}
