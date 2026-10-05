import { Button } from '#/components/ui/button'

interface ProfileActivationProps {
	active: boolean
	confirming: boolean
	busy: boolean
	onActivate: () => void
	onRequestDeactivate: () => void
	onConfirmDeactivate: () => void
	onCancelDeactivate: () => void
}

export function ProfileActivation({
	active,
	confirming,
	busy,
	onActivate,
	onRequestDeactivate,
	onConfirmDeactivate,
	onCancelDeactivate,
}: ProfileActivationProps) {
	if (!active) {
		return (
			<Button size="sm" disabled={busy} onClick={onActivate}>
				Activer
			</Button>
		)
	}
	if (confirming) {
		return (
			<div className="flex flex-wrap items-center gap-2">
				<span className="text-xs text-muted-foreground">
					Arrête la planification, conserve les leads.
				</span>
				<Button
					size="sm"
					variant="destructive"
					disabled={busy}
					onClick={onConfirmDeactivate}
				>
					Confirmer la désactivation
				</Button>
				<Button size="sm" variant="outline" onClick={onCancelDeactivate}>
					Annuler
				</Button>
			</div>
		)
	}
	return (
		<Button
			size="sm"
			variant="outline"
			disabled={busy}
			onClick={onRequestDeactivate}
		>
			Désactiver
		</Button>
	)
}
