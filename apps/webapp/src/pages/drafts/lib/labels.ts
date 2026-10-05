const DRAFT_STATUS_LABELS: Readonly<Record<string, string>> = {
	pending: 'En attente',
	approved: 'Approuvé',
	rejected: 'Rejeté',
}

export function draftStatusLabel(status: string): string {
	return DRAFT_STATUS_LABELS[status] ?? status
}
