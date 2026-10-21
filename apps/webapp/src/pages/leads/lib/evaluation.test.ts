import { describe, expect, it } from 'vitest'
import {
	canReevaluate,
	changedSince,
	evaluatedAtText,
	evaluationLabel,
	evaluationMoves,
	formatClock,
	inspectionProgress,
} from '#/pages/leads/lib/evaluation'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Evaluation } from '#/pages/leads/lib/types'

const EVALUATED = '2026-10-08T09:10:00Z'
const PLANNED = '2026-10-08T09:32:00Z'

function evaluation(overrides: Partial<Evaluation> = {}): Evaluation {
	return {
		stage: 'inspection_running',
		evaluated_at: EVALUATED,
		modules_done: 3,
		modules_available: 7,
		requalify_at: null,
		requalify_reason: null,
		changed_modules: [],
		...overrides,
	}
}

describe('evaluationLabel', () => {
	it('names every stage in French', () => {
		expect(evaluationLabel(evaluation())).toBe(
			'Évalué — inspection en cours (3/7 modules)',
		)
		expect(evaluationLabel(evaluation({ stage: 'inspection_complete' }))).toBe(
			'Évalué — inspection complète',
		)
		expect(
			evaluationLabel(
				evaluation({
					stage: 'requalification_planned',
					requalify_at: PLANNED,
					requalify_reason: 'new_information',
				}),
			),
		).toBe(
			`Réévaluation prévue à ${formatClock(PLANNED)} (nouvelles informations)`,
		)
		expect(
			evaluationLabel(
				evaluation({
					stage: 'requalification_planned',
					requalify_at: PLANNED,
					requalify_reason: 'manual',
				}),
			),
		).toBe(`Réévaluation prévue à ${formatClock(PLANNED)} (demandée)`)
		expect(
			evaluationLabel(
				evaluation({ stage: 'certificate_only', evaluated_at: null }),
			),
		).toBe("Certificat seul — en attente d'une autre preuve")
		expect(
			evaluationLabel(
				evaluation({ stage: 'not_evaluated', evaluated_at: null }),
			),
		).toBe('Non évalué')
		expect(evaluationLabel(evaluation({ stage: 'evaluated' }))).toBe('Évalué')
		expect(evaluationLabel(null)).toBeNull()
		expect(evaluationLabel(undefined)).toBeNull()
	})

	it('writes the planned time as hours and minutes', () => {
		expect(formatClock(PLANNED)).toMatch(/^\d{2}:\d{2}$/)
	})
})

describe('evaluatedAtText', () => {
	it('gives the time of the last evaluation', () => {
		expect(evaluatedAtText(evaluation())).toBe(
			`Dernière évaluation le ${formatDateTime(EVALUATED)}`,
		)
		expect(evaluatedAtText(evaluation({ evaluated_at: null }))).toBeNull()
	})
})

describe('inspectionProgress', () => {
	it('counts the settled modules among the available ones', () => {
		expect(inspectionProgress(evaluation())).toBe('3/7 modules inspectés')
		expect(
			inspectionProgress(evaluation({ modules_done: 0, modules_available: 0 })),
		).toBeNull()
		expect(inspectionProgress(undefined)).toBeNull()
	})
})

describe('changedSince', () => {
	it('lists the modules with new findings since the evaluation', () => {
		expect(
			changedSince(
				evaluation({
					changed_modules: [
						{
							module: 'dns_email',
							label: 'Écosystème DNS et e-mail',
							status: 'done',
							last_run: PLANNED,
							changed_at: PLANNED,
						},
					],
				}),
			),
		).toEqual([
			{
				module: 'dns_email',
				label: 'Écosystème DNS et e-mail',
				at: formatDateTime(PLANNED),
			},
		])
		expect(changedSince(null)).toEqual([])
	})
})

describe('evaluationMoves', () => {
	it('keeps refreshing while the status can still change', () => {
		expect(evaluationMoves(evaluation())).toBe(true)
		expect(
			evaluationMoves(evaluation({ stage: 'requalification_planned' })),
		).toBe(true)
		expect(evaluationMoves(evaluation({ stage: 'not_evaluated' }))).toBe(true)
		expect(evaluationMoves(evaluation({ stage: 'inspection_complete' }))).toBe(
			false,
		)
		expect(evaluationMoves(evaluation({ stage: 'certificate_only' }))).toBe(
			false,
		)
		expect(evaluationMoves(null)).toBe(false)
	})
})

describe('canReevaluate', () => {
	it('allows a reevaluation once the lead was evaluated', () => {
		expect(canReevaluate(evaluation())).toBe(true)
		expect(canReevaluate(evaluation({ stage: 'not_evaluated' }))).toBe(false)
		expect(canReevaluate(evaluation({ stage: 'certificate_only' }))).toBe(false)
		expect(canReevaluate(null)).toBe(false)
	})
})
