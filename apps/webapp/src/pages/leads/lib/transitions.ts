import type { LeadStatus } from './types'

export const LEAD_STATUSES: readonly LeadStatus[] = [
	'new',
	'qualified',
	'validated',
	'rejected',
	'contacted',
	'uncertain',
]

const TRANSITIONS: Readonly<Record<LeadStatus, readonly LeadStatus[]>> = {
	new: ['qualified', 'rejected', 'uncertain'],
	qualified: ['validated', 'rejected', 'new'],
	validated: ['contacted', 'rejected'],
	rejected: ['new'],
	contacted: [],
	uncertain: ['new', 'rejected'],
}

export function nextStatuses(from: LeadStatus): readonly LeadStatus[] {
	return TRANSITIONS[from]
}

export function canMoveTo(from: LeadStatus, to: LeadStatus): boolean {
	return TRANSITIONS[from].includes(to)
}
