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
				Activate
			</Button>
		)
	}
	if (confirming) {
		return (
			<div className="flex flex-wrap items-center gap-2">
				<span className="text-xs text-muted-foreground">
					Stops scheduling, keeps leads.
				</span>
				<Button
					size="sm"
					variant="destructive"
					disabled={busy}
					onClick={onConfirmDeactivate}
				>
					Confirm deactivate
				</Button>
				<Button size="sm" variant="outline" onClick={onCancelDeactivate}>
					Cancel
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
			Deactivate
		</Button>
	)
}
