import type { LeadStatus } from './types'

const LEAD_STATUS_LABELS: Readonly<Record<LeadStatus, string>> = {
	new: 'Nouveau',
	qualified: 'Qualifié',
	validated: 'Validé',
	rejected: 'Rejeté',
	contacted: 'Contacté',
	uncertain: 'Incertain',
}

export function leadStatusLabel(status: LeadStatus): string {
	return LEAD_STATUS_LABELS[status]
}
