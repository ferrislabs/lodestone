const STOP_REASON_LABELS: Readonly<Record<string, string>> = {
	agent_removed: 'Arrêté : agent supprimé',
	agent_disabled: 'Arrêté : agent désactivé',
	profile_deactivated: 'Arrêté : profil désactivé',
	profile_deleted: 'Arrêté : profil supprimé',
}

export function stopReasonLabel(reason: string | null | undefined): string {
	return (reason ? STOP_REASON_LABELS[reason] : undefined) ?? 'Arrêté'
}
