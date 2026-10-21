import { describe, expect, it } from 'vitest'

import { leadStatusLabel } from './labels'
import { LEAD_STATUSES } from './transitions'

describe('leadStatusLabel', () => {
	it('labels every status in french', () => {
		expect(LEAD_STATUSES.map(leadStatusLabel)).toEqual([
			'Nouveau',
			'Qualifié',
			'Validé',
			'Rejeté',
			'Contacté',
			'Incertain',
		])
	})
})
