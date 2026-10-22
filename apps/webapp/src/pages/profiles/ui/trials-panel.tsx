import { formatDateTime } from '#/pages/leads/lib/format'
import {
	formatTrialCost,
	formatTrialProgress,
	trialStatusLabel,
	trialTitle,
} from '#/pages/profiles/lib/trials'
import type { Trial, TrialDetail } from '#/pages/profiles/lib/types'
import { TrialResult } from '#/pages/profiles/ui/trial-result'

interface TrialsPanelProps {
	trials: readonly Trial[]
	loading: boolean
	error: string | null
	selected: string | null
	detail: TrialDetail | null
	loadingDetail: boolean
	detailError: string | null
	onSelect: (trialId: string) => void
	onOpenLead: (leadId: string) => void
}

export function TrialsPanel({
	trials,
	loading,
	error,
	selected,
	detail,
	loadingDetail,
	detailError,
	onSelect,
	onOpenLead,
}: TrialsPanelProps) {
	return (
		<div className="grid gap-4 lg:grid-cols-[22rem_1fr]">
			<div className="flex flex-col gap-2">
				{loading ? (
					<p className="text-sm text-muted-foreground">
						Chargement des essais…
					</p>
				) : null}
				{error ? (
					<p role="alert" className="text-sm text-destructive">
						{error}
					</p>
				) : null}
				{!loading && !error && trials.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucun essai pour l'instant. Modifiez le profil, puis choisissez «
						Essayer sur les leads décidés ».
					</p>
				) : null}
				{trials.length > 0 ? (
					<ul className="flex flex-col divide-y rounded-xl border bg-card">
						{trials.map((trial) => (
							<li
								key={trial.id}
								data-trial={trial.id}
								data-status={trial.status}
							>
								<button
									type="button"
									aria-current={selected === trial.id}
									className="flex w-full flex-col gap-0.5 px-3 py-2 text-left text-sm hover:bg-muted aria-[current=true]:bg-muted"
									onClick={() => onSelect(trial.id)}
								>
									<span className="font-medium">{trialTitle(trial)}</span>
									<span className="text-xs text-muted-foreground">
										{formatDateTime(trial.created_at)}
									</span>
									<span className="text-xs">
										{trialStatusLabel(trial.status)}
									</span>
									<span className="text-xs text-muted-foreground">
										{formatTrialProgress(trial)}
									</span>
									<span className="font-mono text-xs text-muted-foreground">
										{formatTrialCost(trial)}
									</span>
								</button>
							</li>
						))}
					</ul>
				) : null}
			</div>
			<div className="flex min-w-0 flex-col gap-3">
				{selected === null && trials.length > 0 ? (
					<p className="text-sm text-muted-foreground">
						Sélectionner un essai pour l'afficher.
					</p>
				) : null}
				{selected !== null && loadingDetail ? (
					<p className="text-sm text-muted-foreground">
						Chargement de l'essai…
					</p>
				) : null}
				{detailError ? (
					<p role="alert" className="text-sm text-destructive">
						{detailError}
					</p>
				) : null}
				{selected !== null && detail !== null ? (
					<TrialResult detail={detail} onOpenLead={onOpenLead} />
				) : null}
			</div>
		</div>
	)
}
