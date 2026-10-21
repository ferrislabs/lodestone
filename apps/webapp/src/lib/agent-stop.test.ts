import { describe, expect, it } from 'vitest'

import { stopReasonLabel } from './agent-stop'

describe('stopReasonLabel', () => {
	it('names why an agent stopped, in French', () => {
		expect(stopReasonLabel('agent_removed')).toBe('Arrêté : agent supprimé')
		expect(stopReasonLabel('agent_disabled')).toBe('Arrêté : agent désactivé')
		expect(stopReasonLabel('profile_deactivated')).toBe(
			'Arrêté : profil désactivé',
		)
		expect(stopReasonLabel('profile_deleted')).toBe('Arrêté : profil supprimé')
	})

	it('falls back to a plain stop for an unknown or missing reason', () => {
		expect(stopReasonLabel('other')).toBe('Arrêté')
		expect(stopReasonLabel(null)).toBe('Arrêté')
		expect(stopReasonLabel(undefined)).toBe('Arrêté')
	})
})
