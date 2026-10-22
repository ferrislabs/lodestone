import { describe, expect, it } from 'vitest'

import {
	changedLeads,
	changedText,
	decisionLabel,
	errorsText,
	formatCents,
	formatDiscarded,
	formatPrecision,
	formatTrialCost,
	formatTrialProgress,
	isTrialLive,
	leadChanged,
	parseTrialLimits,
	summaryRows,
	trialStatusLabel,
	trialTitle,
} from '#/pages/profiles/lib/trials'
import type { Trial, TrialLead } from '#/pages/profiles/lib/types'

function lead(overrides: Partial<TrialLead> = {}): TrialLead {
	return {
		lead_id: 'l1',
		domain: 'acme.fr',
		label: 'validated',
		current_verdict: 'fiable',
		current_confidence: 80,
		trial_verdict: 'fiable',
		trial_confidence: 75,
		...overrides,
	}
}

function trial(overrides: Partial<Trial> = {}): Trial {
	return {
		id: 't1',
		profile_id: 'p1',
		base_revision: 3,
		status: 'done',
		sample_size: 20,
		budget_cents: 200,
		spent_cents: 12.4,
		leads_tried: 18,
		created_at: '2026-10-20T10:00:00Z',
		...overrides,
	}
}

describe('trialStatusLabel', () => {
	it('names every status in French', () => {
		expect(trialStatusLabel('pending')).toBe('En attente')
		expect(trialStatusLabel('running')).toBe('En cours')
		expect(trialStatusLabel('done')).toBe('Terminé')
		expect(trialStatusLabel('failed')).toBe('Échec')
		expect(trialStatusLabel('budget_exhausted')).toBe('Budget atteint')
	})

	it('keeps an unknown status readable', () => {
		expect(trialStatusLabel('paused')).toBe('paused')
	})
})

describe('isTrialLive', () => {
	it('is live while pending or running only', () => {
		expect(isTrialLive('pending')).toBe(true)
		expect(isTrialLive('running')).toBe(true)
		expect(isTrialLive('done')).toBe(false)
		expect(isTrialLive('failed')).toBe(false)
		expect(isTrialLive('budget_exhausted')).toBe(false)
	})
})

describe('cost formatting', () => {
	it('turns cents into euros with two decimals', () => {
		expect(formatCents(200)).toBe('2,00 €')
		expect(formatCents(5)).toBe('0,05 €')
		expect(formatCents(0)).toBe('0,00 €')
	})

	it('rounds a fractional spend for display', () => {
		expect(formatCents(12.4)).toBe('0,12 €')
		expect(formatCents(149.6)).toBe('1,50 €')
	})

	it('says what a trial spent against its budget', () => {
		expect(formatTrialCost(trial())).toBe('0,12 € sur 2,00 €')
	})
})

describe('trial texts', () => {
	it('names the revision a trial started from', () => {
		expect(trialTitle(trial({ base_revision: 7 }))).toBe(
			'Révision 7 + brouillon',
		)
	})

	it('counts the leads tried against the sample', () => {
		expect(formatTrialProgress(trial())).toBe('18/20 leads')
		expect(formatTrialProgress(trial({ leads_tried: 1, sample_size: 1 }))).toBe(
			'1/1 leads',
		)
	})

	it('counts the rejected leads ranked as discarded', () => {
		expect(
			formatDiscarded({
				reliable: 4,
				reliable_validated: 3,
				rejected: 6,
				rejected_discarded: 5,
			}),
		).toBe('5/6')
	})

	it('words the number of changed verdicts', () => {
		expect(changedText(0)).toBe('Aucun lead ne change de verdict')
		expect(changedText(1)).toBe('1 lead change de verdict')
		expect(changedText(4)).toBe('4 leads changent de verdict')
	})

	it('words the number of leads in error', () => {
		expect(errorsText(1)).toBe('1 lead en erreur')
		expect(errorsText(3)).toBe('3 leads en erreur')
	})

	it('labels the human decision', () => {
		expect(decisionLabel('validated')).toBe('Validé')
		expect(decisionLabel('rejected')).toBe('Rejeté')
		expect(decisionLabel('other')).toBe('other')
	})
})

describe('formatPrecision', () => {
	it('shows a dash without a reliable lead', () => {
		expect(formatPrecision(null)).toBe('—')
		expect(formatPrecision(undefined)).toBe('—')
	})

	it('shows the share of reliable leads that were validated', () => {
		expect(formatPrecision(0.75)).toBe('75 %')
		expect(formatPrecision(1)).toBe('100 %')
		expect(formatPrecision(0)).toBe('0 %')
	})
})

describe('summaryRows', () => {
	it('lines the current version up against the draft', () => {
		expect(
			summaryRows({
				current: {
					reliable: 4,
					reliable_validated: 2,
					precision: 0.5,
					rejected: 6,
					rejected_discarded: 3,
				},
				trial: {
					reliable: 0,
					reliable_validated: 0,
					precision: null,
					rejected: 6,
					rejected_discarded: 5,
				},
				changed: 0,
				errors: 0,
			}),
		).toEqual([
			{ key: 'reliable', label: 'Fiable', current: '4', trial: '0' },
			{ key: 'validated', label: 'dont validés', current: '2', trial: '0' },
			{ key: 'precision', label: 'Précision', current: '50 %', trial: '—' },
			{
				key: 'discarded',
				label: 'Rejetés classés écartés',
				current: '3/6',
				trial: '5/6',
			},
		])
	})
})

describe('changed leads', () => {
	it('flags a different verdict', () => {
		expect(leadChanged(lead({ trial_verdict: 'ecarte' }))).toBe(true)
		expect(leadChanged(lead())).toBe(false)
	})

	it('counts a missing verdict on one side as a change', () => {
		expect(leadChanged(lead({ current_verdict: null }))).toBe(true)
		expect(leadChanged(lead({ trial_verdict: undefined }))).toBe(true)
		expect(
			leadChanged(lead({ current_verdict: null, trial_verdict: null })),
		).toBe(false)
	})

	it('never flags a lead that failed', () => {
		expect(
			leadChanged(lead({ trial_verdict: null, error: 'model call failed' })),
		).toBe(false)
	})

	it('keeps the changed leads in order', () => {
		const leads = [
			lead({ lead_id: 'a' }),
			lead({ lead_id: 'b', trial_verdict: 'a_verifier' }),
			lead({ lead_id: 'c', trial_verdict: 'ecarte' }),
		]
		expect(changedLeads(leads).map((item) => item.lead_id)).toEqual(['b', 'c'])
	})
})

describe('parseTrialLimits', () => {
	it('reads whole numbers inside the limits', () => {
		expect(parseTrialLimits('20', '200')).toEqual({
			ok: true,
			sampleSize: 20,
			budgetCents: 200,
		})
		expect(parseTrialLimits('1', '1')).toEqual({
			ok: true,
			sampleSize: 1,
			budgetCents: 1,
		})
		expect(parseTrialLimits(' 50 ', '2000')).toEqual({
			ok: true,
			sampleSize: 50,
			budgetCents: 2000,
		})
	})

	it('refuses a sample outside 1 to 50', () => {
		for (const sample of ['0', '51', '', 'abc', '2.5']) {
			const result = parseTrialLimits(sample, '200')
			expect(result.ok).toBe(false)
			if (!result.ok) {
				expect(result.message).toBe(
					"La taille de l'échantillon doit être un entier de 1 à 50",
				)
			}
		}
	})

	it('refuses a budget outside 1 to 2000', () => {
		for (const budget of ['0', '2001', '', '-5', '1e2']) {
			const result = parseTrialLimits('20', budget)
			expect(result.ok).toBe(false)
			if (!result.ok) {
				expect(result.message).toBe(
					'Le budget doit être un entier de 1 à 2000 centimes',
				)
			}
		}
	})
})
