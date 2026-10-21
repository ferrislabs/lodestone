import {
	BanIcon,
	CheckIcon,
	CircleDashedIcon,
	ClockIcon,
	LoaderCircleIcon,
} from 'lucide-react'

import { cn } from '#/lib/utils'
import { formatDateTime } from '#/pages/leads/lib/format'
import {
	nextText,
	type PipelineStage,
	type Stage,
	type StageStatus,
	stageStatusLabel,
} from '#/pages/leads/lib/timeline'

const TONES: Readonly<Record<StageStatus, string>> = {
	done: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	running: 'border-indigo-300 bg-indigo-50 text-indigo-700',
	waiting: 'border-amber-200 bg-amber-50 text-amber-700',
	blocked: 'border-red-200 bg-red-50 text-red-700',
	skipped: 'border-slate-200 bg-slate-50 text-slate-500',
}

function StatusIcon({ status }: { status: StageStatus }) {
	switch (status) {
		case 'done':
			return <CheckIcon aria-hidden className="size-3.5" />
		case 'running':
			return <LoaderCircleIcon aria-hidden className="size-3.5 animate-spin" />
		case 'waiting':
			return <ClockIcon aria-hidden className="size-3.5" />
		case 'blocked':
			return <BanIcon aria-hidden className="size-3.5" />
		case 'skipped':
			return <CircleDashedIcon aria-hidden className="size-3.5" />
	}
}

interface PipelineStepperProps {
	stages: readonly PipelineStage[]
	current: Stage | null
	now: number
	error: string | null
}

export function PipelineStepper({
	stages,
	current,
	now,
	error,
}: PipelineStepperProps) {
	const focus = stages.find((stage) => stage.stage === current) ?? null
	const next = focus ? nextText(focus, now) : null
	return (
		<section
			data-section="pipeline"
			aria-label="Avancement du traitement"
			className="flex flex-col gap-3 rounded-lg border p-4"
		>
			{error ? (
				<p role="alert" className="text-sm text-destructive">
					Avancement indisponible : {error}
				</p>
			) : null}
			<ol className="flex flex-wrap items-center gap-2">
				{stages.map((stage, index) => (
					<li
						key={stage.stage}
						data-stage={stage.stage}
						data-status={stage.status}
						aria-current={stage.stage === current ? 'step' : undefined}
						title={stage.reason ?? undefined}
						className="flex items-center gap-2"
					>
						{index > 0 ? (
							<span aria-hidden className="h-px w-3 bg-border" />
						) : null}
						<span
							className={cn(
								'inline-flex items-center gap-1 rounded-md border px-2 py-1 text-xs',
								TONES[stage.status],
								stage.stage === current && 'ring-2 ring-offset-1 ring-primary',
							)}
						>
							<StatusIcon status={stage.status} />
							<span className="font-medium">{stage.label}</span>
							<span className="sr-only">
								{' '}
								: {stageStatusLabel(stage.status)}
							</span>
						</span>
					</li>
				))}
			</ol>
			{focus ? (
				<div className="flex flex-col gap-0.5 text-sm" data-current-stage>
					<p>
						<span className="font-medium">{focus.label}</span> —{' '}
						{stageStatusLabel(focus.status).toLowerCase()}
						{focus.reason ? ` : ${focus.reason}` : ''}
					</p>
					{focus.since ? (
						<p className="text-xs text-muted-foreground">
							Depuis le {formatDateTime(focus.since)}
						</p>
					) : null}
					{next ? <p className="text-xs">{next}</p> : null}
				</div>
			) : stages.length > 0 ? (
				<p className="text-sm text-muted-foreground">
					Traitement terminé pour ce lead.
				</p>
			) : null}
		</section>
	)
}
