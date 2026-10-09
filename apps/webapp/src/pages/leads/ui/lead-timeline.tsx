import { cn } from '#/lib/utils'
import { formatDateTime } from '#/pages/leads/lib/format'
import {
	actorText,
	detailText,
	type EventStatus,
	eventLabel,
	eventStatusLabel,
	formatDuration,
	type InFlight,
	roundTitle,
	stepLabel,
	type TimelineEvent,
	type TimelineRound,
	type TimelineStep,
} from '#/pages/leads/lib/timeline'

const STATUS_TONES: Readonly<Record<EventStatus, string>> = {
	started: 'border-indigo-200 bg-indigo-50 text-indigo-700',
	succeeded: 'border-emerald-200 bg-emerald-50 text-emerald-700',
	failed: 'border-red-200 bg-red-50 text-red-700',
	skipped: 'border-slate-200 bg-slate-50 text-slate-600',
	waiting: 'border-amber-200 bg-amber-50 text-amber-700',
}

function Chip({ className, text }: { className: string; text: string }) {
	return (
		<span
			className={cn(
				'inline-flex w-fit items-center rounded-md border px-1.5 py-0.5 text-xs',
				className,
			)}
		>
			{text}
		</span>
	)
}

function EventItem({ event }: { event: TimelineEvent }) {
	const actor = actorText(event.actor)
	const duration = formatDuration(event.duration_ms)
	const detail = detailText(event)
	return (
		<li
			data-event={event.kind}
			data-step={event.step}
			className="flex flex-col gap-1 border-l-2 py-1 pl-3"
		>
			<div className="flex flex-wrap items-center gap-2 text-sm">
				<time
					dateTime={event.started_at}
					className="text-xs text-muted-foreground"
				>
					{formatDateTime(event.started_at)}
				</time>
				<span className="text-xs text-muted-foreground">
					{stepLabel(event.step)}
				</span>
				<span className="font-medium">{eventLabel(event)}</span>
				{actor ? <span className="text-muted-foreground">{actor}</span> : null}
				<Chip
					className={STATUS_TONES[event.status]}
					text={eventStatusLabel(event.status)}
				/>
				{duration ? (
					<span className="text-xs text-muted-foreground">{duration}</span>
				) : null}
				{event.reconstructed ? (
					<Chip
						className="border-slate-200 bg-slate-50 text-slate-500"
						text="reconstitué"
					/>
				) : null}
			</div>
			{event.error ? (
				<p className="text-xs text-destructive">{event.error}</p>
			) : null}
			{detail ? (
				<details className="text-xs">
					<summary className="cursor-pointer text-muted-foreground">
						Détails
					</summary>
					<pre className="mt-1 overflow-x-auto rounded bg-muted p-2">
						{detail}
					</pre>
				</details>
			) : null}
		</li>
	)
}

interface LeadTimelineProps {
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	rounds: readonly TimelineRound[]
	inFlight: readonly InFlight[]
	steps: readonly TimelineStep[]
	selected: readonly TimelineStep[]
	onToggleStep: (step: TimelineStep) => void
	onClearSteps: () => void
}

export function LeadTimeline({
	state,
	errorMessage,
	rounds,
	inFlight,
	steps,
	selected,
	onToggleStep,
	onClearSteps,
}: LeadTimelineProps) {
	return (
		<section
			data-section="timeline"
			aria-label="Chronologie"
			className="flex flex-col gap-3 rounded-lg border p-4"
		>
			<h2 className="text-sm font-semibold">Chronologie</h2>
			{state === 'loading' ? (
				<p className="text-sm text-muted-foreground" aria-busy="true">
					Chargement de la chronologie…
				</p>
			) : null}
			{state === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					Chronologie indisponible : {errorMessage}
				</p>
			) : null}
			{inFlight.length > 0 ? (
				<div className="flex flex-col gap-1" data-in-flight>
					<p className="text-sm font-medium">En cours</p>
					<ul className="flex flex-col gap-0.5 text-sm">
						{inFlight.map((running) => (
							<li
								key={`${running.step}-${running.started_at}-${actorText(running.actor) ?? ''}`}
							>
								{stepLabel(running.step)}
								{actorText(running.actor)
									? ` — ${actorText(running.actor)}`
									: ''}
								<span className="text-xs text-muted-foreground">
									{' '}
									(depuis {formatDateTime(running.started_at)})
								</span>
							</li>
						))}
					</ul>
				</div>
			) : null}
			{steps.length > 0 ? (
				<fieldset className="flex flex-wrap gap-1 border-0 p-0">
					<legend className="sr-only">Filtrer par étape</legend>
					<button
						type="button"
						aria-pressed={selected.length === 0}
						onClick={onClearSteps}
						className={cn(
							'rounded-full border px-2 py-0.5 text-xs',
							selected.length === 0 && 'bg-primary text-primary-foreground',
						)}
					>
						Toutes
					</button>
					{steps.map((step) => (
						<button
							key={step}
							type="button"
							aria-pressed={selected.includes(step)}
							onClick={() => onToggleStep(step)}
							className={cn(
								'rounded-full border px-2 py-0.5 text-xs',
								selected.includes(step) && 'bg-primary text-primary-foreground',
							)}
						>
							{stepLabel(step)}
						</button>
					))}
				</fieldset>
			) : null}
			{state === 'ready' && rounds.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					Aucun événement enregistré.
				</p>
			) : null}
			{rounds.map((round) => (
				<div
					key={round.number}
					data-round={round.number}
					className="flex flex-col gap-1"
				>
					<h3 className="flex flex-wrap items-center gap-2 text-sm font-medium">
						{roundTitle(round)}
						{round.reconstructed ? (
							<Chip
								className="border-slate-200 bg-slate-50 text-slate-500"
								text="reconstitué"
							/>
						) : null}
					</h3>
					<ol className="flex flex-col gap-1">
						{round.events.map((event) => (
							<EventItem key={event.id} event={event} />
						))}
					</ol>
				</div>
			))}
		</section>
	)
}
