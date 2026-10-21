import { FieldErrors } from '#/components/schema-form/field-frame'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import type { Problem } from '#/lib/schema-form'
import {
	type Escalation,
	escalationProblems,
	type ProviderOption,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { ProviderSelect } from '#/pages/profiles/ui/provider-select'

interface EscalationPanelProps {
	escalation: Escalation
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	actions: QualificationActions
}

function BandInput({
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
		<div className="flex max-w-56 flex-col gap-1">
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

export function EscalationPanel({
	escalation,
	problems,
	providers,
	actions,
}: EscalationPanelProps) {
	const local = escalationProblems(escalation)
	const server = problems.filter(
		(problem) =>
			problem.path.startsWith('escalation') &&
			!local.some((item) => item.path === problem.path),
	)
	const messages = [...local, ...server]
	const providerProblems = messages.filter(
		(message) => message.path === 'escalation.provider',
	)
	const invalidFor = (path: string) =>
		messages.some((message) => message.path === path)
	const enabled = escalation.provider !== ''
	return (
		<section
			aria-label="Escalade"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-sm"
		>
			<div className="flex flex-col gap-1">
				<h2 className="text-base font-semibold">Escalade</h2>
				<p className="text-xs text-muted-foreground">
					Un lead dont la confiance tombe dans la bande grise est réévalué par
					un fournisseur plus puissant : chaque agent actif est rejoué avec un
					seul échantillon à température 0, et les nouveaux résultats remplacent
					les anciens. Un lead n'est escaladé qu'une fois par révision du
					profil.
				</p>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="escalation-provider">Fournisseur d'escalade</Label>
				<ProviderSelect
					id="escalation-provider"
					value={escalation.provider}
					providers={providers}
					emptyLabel="Désactivée"
					invalid={providerProblems.length > 0}
					onChange={actions.setEscalationProvider}
				/>
				<FieldErrors id="escalation-provider" problems={providerProblems} />
			</div>
			<div className="flex flex-col gap-1.5">
				<h3 className="text-sm font-semibold">Bande grise</h3>
				<div className="flex flex-wrap gap-4">
					<BandInput
						id="escalation-band-low"
						label="À partir de"
						value={escalation.bandLow}
						invalid={invalidFor('escalation.band_low')}
						onChange={actions.setBandLow}
					/>
					<BandInput
						id="escalation-band-high"
						label="Jusqu'à (exclu)"
						value={escalation.bandHigh}
						invalid={invalidFor('escalation.band_high')}
						onChange={actions.setBandHigh}
					/>
				</div>
				<p className="text-xs text-muted-foreground">
					{enabled
						? `Escalade des leads dont la confiance est de ${escalation.bandLow ?? '?'} % à moins de ${escalation.bandHigh ?? '?'} %.`
						: "L'escalade est désactivée : la bande grise n'a aucun effet."}
				</p>
			</div>
			{messages
				.filter((message) => message.path !== 'escalation.provider')
				.map((message) => (
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
