import { describe, expect, it } from 'vitest'

import {
	calibrationKindLabel,
	calibrationStatusLabel,
	curveLine,
	curvePoints,
	formatBrier,
	recalibrationNotice,
	visibleProfiles,
} from '#/pages/quality/lib/calibration'
import type { ProfileCalibration } from '#/pages/quality/lib/types'

describe('calibrationStatusLabel', () => {
	it('names the three states in French', () => {
		expect(calibrationStatusLabel({ status: 'active', n: 80 }, 50)).toBe(
			'Active',
		)
		expect(calibrationStatusLabel({ status: 'disabled', n: 80 }, 50)).toBe(
			'Désactivée',
		)
		expect(calibrationStatusLabel({ status: 'insufficient', n: 12 }, 50)).toBe(
			'Pas assez de décisions (12/50)',
		)
	})
})

describe('calibrationKindLabel', () => {
	it('labels the agent kinds', () => {
		expect(calibrationKindLabel('scoring')).toBe('Scoring')
		expect(calibrationKindLabel('qualification')).toBe('Qualification')
		expect(calibrationKindLabel('autre')).toBe('autre')
	})
})

describe('formatBrier', () => {
	it('rounds to three decimals and shows a dash when absent', () => {
		expect(formatBrier(0.31234)).toBe('0,312')
		expect(formatBrier(0)).toBe('0')
		expect(formatBrier(null)).toBe('—')
		expect(formatBrier(undefined)).toBe('—')
	})
})

describe('curvePoints', () => {
	it('puts the prediction on x and the observation on y, upwards', () => {
		expect(
			curvePoints([
				{ n: 4, predicted: 0.85, observed: 0.6 },
				{ n: 2, predicted: 0.15, observed: 0.1 },
			]),
		).toEqual([
			{ x: 15, y: 90, n: 2 },
			{ x: 85, y: 40, n: 4 },
		])
	})

	it('keeps the points inside the frame and joins them in order', () => {
		const points = curvePoints([
			{ n: 1, predicted: 1.4, observed: -0.2 },
			{ n: 1, predicted: 0, observed: 1 },
		])
		expect(points).toEqual([
			{ x: 0, y: 0, n: 1 },
			{ x: 100, y: 100, n: 1 },
		])
		expect(curveLine(points)).toBe('0,0 100,100')
		expect(curvePoints([])).toEqual([])
	})
})

describe('visibleProfiles', () => {
	const profiles: ProfileCalibration[] = [
		{ profile_id: 'a', name: 'A', agents: [] },
		{ profile_id: 'b', name: 'B', agents: [] },
	]

	it('keeps every profile without a selection and the chosen one otherwise', () => {
		expect(visibleProfiles(profiles, '')).toHaveLength(2)
		expect(visibleProfiles(profiles, 'b').map((p) => p.name)).toEqual(['B'])
		expect(visibleProfiles(profiles, 'zzz')).toEqual([])
	})
})

describe('recalibrationNotice', () => {
	it('tells whether a run was queued or was already waiting', () => {
		expect(recalibrationNotice(true)).toContain('Recalcul programmé')
		expect(recalibrationNotice(false)).toBe('Un recalcul est déjà en attente.')
	})
})
