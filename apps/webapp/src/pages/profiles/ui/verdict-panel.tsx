import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import type { Problem } from '#/lib/schema-form'
import {
	type Verdict,
	verdictBands,
	verdictProblems,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'

interface VerdictPanelProps {
	verdict: Verdict
	problems: readonly Problem[]
	actions: QualificationActions
}

function PercentInput({
	id,
	label,
	value,
	invalid,
	onChange,
}: {
	id: string
	label: string
	value: number | null
	invalid: boolean
	onChange: (percent: number | null) => void
}) {
	return (
		<div className="flex flex-col gap-1">
			<Label htmlFor={id}>{label}</Label>
			<div className="relative max-w-40">
				<Input
					id={id}
					type="number"
					min={0}
					max={100}
					step={1}
					inputMode="numeric"
					className="pr-8"
					aria-invalid={invalid}
					value={value ?? ''}
					onChange={(event) => {
						const parsed = event.currentTarget.valueAsNumber
						onChange(Number.isNaN(parsed) ? null : parsed)
					}}
				/>
				<span
					aria-hidden="true"
					className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-sm text-muted-foreground"
				>
					%
				</span>
			</div>
		</div>
	)
}

export function VerdictPanel({
	verdict,
	problems,
	actions,
}: VerdictPanelProps) {
	const local = verdictProblems(verdict)
	const server = problems.filter(
		(problem) =>
			problem.path.startsWith('verdict') &&
			!local.some((item) => item.path === problem.path),
	)
	const messages = [...local, ...server]
	const bands = verdictBands(verdict)
	const invalidFor = (path: string) =>
		messages.some((message) => message.path === path)
	return (
		<section
			aria-label="Verdicts"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-sm"
		>
			<div className="flex flex-col gap-1">
				<h2 className="text-base font-semibold">Verdicts</h2>
				<p className="text-xs text-muted-foreground">
					La confiance du lead, en pourcentage, décide de son verdict : fiable,
					à vérifier ou écarté.
				</p>
			</div>
			<div className="flex flex-wrap gap-4">
				<PercentInput
					id="verdict-reliable-from"
					label="Fiable à partir de"
					value={verdict.reliableFrom}
					invalid={invalidFor('verdict.reliable_from')}
					onChange={actions.setReliableFrom}
				/>
				<PercentInput
					id="verdict-discard-below"
					label="Écarté sous"
					value={verdict.discardBelow}
					invalid={invalidFor('verdict.discard_below')}
					onChange={actions.setDiscardBelow}
				/>
			</div>
			{bands ? (
				<div className="flex flex-col gap-1.5">
					<div
						data-testid="verdict-bar"
						className="flex h-3 w-full overflow-hidden rounded-full"
					>
						<span
							data-band="discarded"
							style={{ width: `${bands.discarded}%` }}
							className="bg-red-500/70"
						/>
						<span
							data-band="to-check"
							style={{ width: `${bands.toCheck}%` }}
							className="bg-amber-500/70"
						/>
						<span
							data-band="reliable"
							style={{ width: `${bands.reliable}%` }}
							className="bg-emerald-500/70"
						/>
					</div>
					<p className="text-xs text-muted-foreground">
						Écarté : moins de {verdict.discardBelow} % · À vérifier : de{' '}
						{verdict.discardBelow} % à moins de {verdict.reliableFrom} % ·
						Fiable : {verdict.reliableFrom} % et plus
					</p>
				</div>
			) : null}
			{messages.map((message) => (
				<p
					key={`${message.path}-${message.message}`}
					role="alert"
					className="text-xs text-destructive"
				>
					{message.message}
				</p>
			))}
		</section>
	)
}
