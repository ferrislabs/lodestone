import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { signalKindLabel } from '#/lib/labels'
import type { Problem } from '#/lib/schema-form'
import {
	EVIDENCE_WEIGHT_KEYS,
	type EvidenceSettings,
	type EvidenceWeightKey,
	evidenceProblems,
	HALF_LIFE_KEYS,
	type HalfLifeKey,
	MAX_CALIBRATION_FLOOR_PERCENT,
	MAX_EVIDENCE_WEIGHT,
	MAX_HALF_LIFE_DAYS,
	MIN_HALF_LIFE_DAYS,
	type Verdict,
	verdictBands,
	verdictProblems,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'

interface VerdictPanelProps {
	verdict: Verdict
	evidence: EvidenceSettings
	problems: readonly Problem[]
	actions: QualificationActions
}

function NumberInput({
	id,
	label,
	help,
	value,
	invalid,
	min = 0,
	max,
	suffix,
	onChange,
}: {
	id: string
	label: string
	help?: string
	value: number | null
	invalid: boolean
	min?: number
	max?: number
	suffix?: string
	onChange: (value: number | null) => void
}) {
	return (
		<div className="flex max-w-56 flex-col gap-1">
			<Label htmlFor={id}>{label}</Label>
			<div className="relative max-w-40">
				<Input
					id={id}
					type="number"
					min={min}
					max={max}
					step={1}
					inputMode="numeric"
					className={suffix ? 'pr-8' : undefined}
					aria-invalid={invalid}
					aria-describedby={help ? `${id}-help` : undefined}
					value={value ?? ''}
					onChange={(event) => {
						const parsed = event.currentTarget.valueAsNumber
						onChange(Number.isNaN(parsed) ? null : parsed)
					}}
				/>
				{suffix ? (
					<span
						aria-hidden="true"
						className="pointer-events-none absolute inset-y-0 right-3 flex items-center text-sm text-muted-foreground"
					>
						{suffix}
					</span>
				) : null}
			</div>
			{help ? (
				<p id={`${id}-help`} className="text-xs text-muted-foreground">
					{help}
				</p>
			) : null}
		</div>
	)
}

function PercentInput(props: {
	id: string
	label: string
	help?: string
	value: number | null
	invalid: boolean
	max?: number
	onChange: (percent: number | null) => void
}) {
	return <NumberInput {...props} max={props.max ?? 100} suffix="%" />
}

const WEIGHT_LABELS: Readonly<Record<EvidenceWeightKey, string>> = {
	volume: 'Poids : volume de signaux',
	diversity: 'Poids : diversité des sources',
	environment: 'Poids : environnement',
	strength: 'Poids : force des signaux',
}

const DEFAULT_HALF_LIFE_LABEL = 'Autres signaux'

function halfLifeLabel(key: HalfLifeKey): string {
	return key === 'default' ? DEFAULT_HALF_LIFE_LABEL : signalKindLabel(key)
}

export function VerdictPanel({
	verdict,
	evidence,
	problems,
	actions,
}: VerdictPanelProps) {
	const local = [...verdictProblems(verdict), ...evidenceProblems(evidence)]
	const server = problems.filter(
		(problem) =>
			(problem.path.startsWith('verdict') ||
				problem.path.startsWith('evidence')) &&
			!local.some((item) => item.path === problem.path),
	)
	const messages = [...local, ...server]
	const bands = verdictBands(verdict)
	const invalidFor = (path: string) =>
		messages.some((message) => message.path === path)
	return (
		<section
			aria-label="Verdicts et preuves"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-sm"
		>
			<div className="flex flex-col gap-1">
				<h2 className="text-base font-semibold">Verdicts et preuves</h2>
				<p className="text-xs text-muted-foreground">
					La confiance du lead, en pourcentage, décide de son verdict : fiable,
					à vérifier ou écarté. Elle combine l'avis des agents et la solidité
					des preuves.
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
			<div className="flex flex-col gap-2 border-t pt-3">
				<h3 className="text-sm font-semibold">Conditions pour être fiable</h3>
				<p className="text-xs text-muted-foreground">
					Un lead n'est fiable que si deux angles d'analyse indépendants
					l'appuient (deux agents, ou un agent avec deux axes) et si ses preuves
					sont assez solides. Sinon il reste « À vérifier ».
				</p>
				<div className="flex flex-wrap gap-4">
					<PercentInput
						id="verdict-min-evidence"
						label="Preuves minimales"
						help="Solidité des preuves requise pour être fiable."
						value={evidence.minEvidence}
						invalid={invalidFor('verdict.min_evidence')}
						onChange={actions.setMinEvidence}
					/>
					<PercentInput
						id="verdict-calibration-floor"
						label="Plancher de calibration"
						help="Chaque probabilité est ramenée entre ce plancher et son complément à 1. 0 % désactive la calibration."
						value={evidence.calibrationFloor}
						invalid={invalidFor('verdict.calibration_floor')}
						max={MAX_CALIBRATION_FLOOR_PERCENT}
						onChange={actions.setCalibrationFloor}
					/>
				</div>
			</div>
			<div className="flex flex-col gap-2 border-t pt-3">
				<h3 className="text-sm font-semibold">Solidité des preuves</h3>
				<p className="text-xs text-muted-foreground">
					Mesurée par le code à partir des signaux du lead et de ceux des autres
					domaines de la même entreprise : volume de signaux indépendants (un
					même signal vu par la même source compte une fois, et vieillit),
					diversité des sources et des types de signaux, environnement des hôtes
					et force des signaux (moyenne des trois signaux indépendants les plus
					forts, chacun avec la force donnée par sa source et diminué selon son
					âge). Confiance = avis des agents × ((1 − influence) + influence ×
					solidité).
				</p>
				<div className="flex flex-wrap gap-4">
					<PercentInput
						id="evidence-influence"
						label="Influence des preuves"
						help="0 % ignore les preuves, 100 % multiplie l'avis par elles."
						value={evidence.influence}
						invalid={invalidFor('evidence.influence')}
						onChange={actions.setInfluence}
					/>
				</div>
				<div className="flex flex-wrap gap-4">
					{EVIDENCE_WEIGHT_KEYS.map((key) => (
						<NumberInput
							key={key}
							id={`evidence-weight-${key}`}
							label={WEIGHT_LABELS[key]}
							value={evidence.weights[key]}
							invalid={invalidFor(`evidence.weights.${key}`)}
							max={MAX_EVIDENCE_WEIGHT}
							onChange={(weight) => actions.setEvidenceWeight(key, weight)}
						/>
					))}
				</div>
			</div>
			<fieldset className="flex flex-col gap-2 border-t pt-3">
				<legend className="text-sm font-semibold">
					Demi-vie des signaux (jours)
				</legend>
				<p className="text-xs text-muted-foreground">
					Un signal de cet âge compte pour moitié dans le volume et la force des
					preuves. « Autres signaux » s'applique aux types non listés.
				</p>
				<div className="flex flex-wrap gap-4">
					{HALF_LIFE_KEYS.map((key) => (
						<NumberInput
							key={key}
							id={`evidence-half-life-${key}`}
							label={halfLifeLabel(key)}
							value={evidence.halfLives[key]}
							invalid={invalidFor(`evidence.half_life_days.${key}`)}
							min={MIN_HALF_LIFE_DAYS}
							max={MAX_HALF_LIFE_DAYS}
							suffix="j"
							onChange={(days) => actions.setHalfLife(key, days)}
						/>
					))}
				</div>
			</fieldset>
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
