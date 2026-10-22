import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'
import type { Problem } from '#/pages/profiles/lib/types'
import { ProfileProblems } from '#/pages/profiles/ui/profile-problems'

interface ProfileEditorProps {
	form: ReactNode
	problems: readonly Problem[]
	notice: string | null
	dirty: boolean
	saving: boolean
	saveLabel: string
	blockedReason?: string | null
	confirmingReset: boolean
	trialPanel?: ReactNode
	onTry?: () => void
	onSave: () => void
	onRequestReset: () => void
	onConfirmReset: () => void
	onCancelReset: () => void
}

export function ProfileEditor({
	form,
	problems,
	notice,
	dirty,
	saving,
	saveLabel,
	blockedReason = null,
	confirmingReset,
	trialPanel = null,
	onTry,
	onSave,
	onRequestReset,
	onConfirmReset,
	onCancelReset,
}: ProfileEditorProps) {
	return (
		<form
			noValidate
			className="flex flex-col gap-3"
			onSubmit={(event) => {
				event.preventDefault()
				onSave()
			}}
		>
			<ProfileProblems
				problems={problems}
				title="Problèmes non liés à un champ"
			/>
			{form}
			{notice ? (
				<output className="block text-sm text-muted-foreground">
					{notice}
				</output>
			) : null}
			{dirty ? (
				<div
					data-testid="save-bar"
					className="sticky bottom-0 z-10 -mx-4 flex flex-col gap-3 border-t bg-background px-4 py-3 md:-mx-8 md:px-8"
				>
					{trialPanel}
					<div className="flex flex-wrap items-center justify-between gap-3">
						<div className="flex flex-col text-sm">
							<span className="font-medium">
								Modifications non enregistrées
							</span>
							{blockedReason ? (
								<span className="text-xs text-muted-foreground">
									{blockedReason}
								</span>
							) : null}
						</div>
						<div className="flex flex-wrap items-center gap-2">
							{confirmingReset ? (
								<>
									<Button
										type="button"
										variant="destructive"
										size="sm"
										disabled={saving}
										onClick={onConfirmReset}
									>
										Confirmer l'annulation
									</Button>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={onCancelReset}
									>
										Conserver mes modifications
									</Button>
								</>
							) : (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									disabled={saving}
									onClick={onRequestReset}
								>
									Annuler les modifications
								</Button>
							)}
							{onTry ? (
								<Button
									type="button"
									variant="outline"
									disabled={saving}
									onClick={onTry}
								>
									Essayer sur les leads décidés
								</Button>
							) : null}
							<Button type="submit" disabled={saving || blockedReason !== null}>
								{saving ? 'Enregistrement…' : saveLabel}
							</Button>
						</div>
					</div>
				</div>
			) : null}
		</form>
	)
}
