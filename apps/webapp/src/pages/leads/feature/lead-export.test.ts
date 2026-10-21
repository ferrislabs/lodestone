import { describe, expect, it } from 'vitest'
import { exportUrl } from './lead-export'

describe('exportUrl', () => {
	it('carries the format and the active filters', () => {
		const url = exportUrl('http://api.test/', 'p 1', 'csv', {
			status: 'new',
			verdict: 'fiable,a_verifier,en_attente',
			min_confidence: 40,
			min_score: 2,
			sort: 'confidence_desc',
		})
		expect(url).toBe(
			'http://api.test/api/v1/profiles/p%201/leads/export?format=csv&status=new&verdict=fiable%2Ca_verifier%2Cen_attente&min_confidence=40&min_score=2&sort=confidence_desc',
		)
	})
})
