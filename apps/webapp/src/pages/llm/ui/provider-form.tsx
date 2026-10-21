import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { providerKindLabel, reasoningLabel } from '#/pages/llm/lib/labels'
import {
	type KeyMode,
	PROVIDER_KINDS,
	type ProviderFieldName,
	type ProviderFormValues,
	REASONING_EFFORTS,
} from '#/pages/llm/lib/types'

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

const REASONING_HELP =
	"Pour les modèles qui raisonnent avant de répondre (Ollama, OpenAI o-series…). « Désactivé » accélère les réponses et évite les réponses vides. La réflexion du modèle n'est jamais affichée ni enregistrée : seule la réponse finale est utilisée. Laissez « Par défaut (non envoyé) » pour un modèle sans raisonnement réglable (ex. Ministral)."

const CONCURRENCY_HELP =
	"Appels au modèle traités en même temps par ce fournisseur, de 1 à 32, toutes analyses confondues. Laissé vide : 2 pour un serveur compatible OpenAI local (localhost), 4 sinon. Avec Ollama, ne dépassez pas sa variable OLLAMA_NUM_PARALLEL : les requêtes en trop attendent côté serveur et risquent d'expirer."

const GENERATION_INTRO =
	"Laissés vides, ces paramètres ne sont pas envoyés et le modèle garde ses valeurs par défaut. Anthropic n'utilise que les tokens maximum et la température ; les pénalités concernent les fournisseurs compatibles OpenAI (Ollama, Mistral…)."

interface GenerationField {
	field: ProviderFieldName
	label: string
	placeholder: string
	inputMode: 'numeric' | 'decimal'
	help: string
}

const GENERATION_FIELDS: readonly GenerationField[] = [
	{
		field: 'max_tokens',
		label: 'Tokens maximum',
		placeholder: 'ex. 4096',
		inputMode: 'numeric',
		help: 'Plafond de tokens générés par réponse, de 1 à 200 000. Remplace la valeur par défaut de Lodestone.',
	},
	{
		field: 'temperature',
		label: 'Température',
		placeholder: 'ex. 0,2',
		inputMode: 'decimal',
		help: 'De 0 à 2. Plus elle est basse, plus les réponses sont stables et reproductibles.',
	},
	{
		field: 'frequency_penalty',
		label: 'Pénalité de fréquence',
		placeholder: 'ex. 0,3',
		inputMode: 'decimal',
		help: 'De -2 à 2. Une valeur positive décourage la répétition des mêmes mots.',
	},
	{
		field: 'presence_penalty',
		label: 'Pénalité de présence',
		placeholder: 'ex. 0,2',
		inputMode: 'decimal',
		help: 'De -2 à 2. Une valeur positive pousse le modèle à aborder de nouveaux sujets.',
	},
	{
		field: 'repeat_penalty',
		label: 'Pénalité de répétition',
		placeholder: 'ex. 1,3',
		inputMode: 'decimal',
		help: "De 1 à 2. 1 n'a aucun effet, 1,1 à 1,3 limite les redites. Ollama l'ignore sur son point d'accès compatible OpenAI : elle est alors envoyée comme pénalité de fréquence (valeur moins 1), sauf si celle-ci est renseignée.",
	},
]

const KEY_MODES: { mode: KeyMode; label: string }[] = [
	{ mode: 'keep', label: 'Conserver la clé actuelle' },
	{ mode: 'replace', label: 'Remplacer la clé' },
	{ mode: 'remove', label: 'Supprimer la clé' },
]

interface ProviderFormProps {
	title: string
	defaultConcurrency: number
	editing: boolean
	hasApiKey: boolean
	values: ProviderFormValues
	fieldErrors: Partial<Record<ProviderFieldName, string>>
	formError: string | null
	keyMode: KeyMode
	saving: boolean
	dirty: boolean
	onChange: (field: ProviderFieldName, value: string) => void
	onKeyModeChange: (mode: KeyMode) => void
	onSave: () => void
	onBack: () => void
}

interface TextFieldProps {
	field: ProviderFieldName
	label: string
	values: ProviderFormValues
	errors: Partial<Record<ProviderFieldName, string>>
	placeholder?: string
	inputMode?: 'numeric' | 'decimal'
	help?: string
	onChange: (field: ProviderFieldName, value: string) => void
}

function TextField({
	field,
	label,
	values,
	errors,
	placeholder,
	inputMode,
	help,
	onChange,
}: TextFieldProps) {
	const error = errors[field]
	const describedBy = [
		help ? `provider-${field}-help` : null,
		error ? `provider-${field}-error` : null,
	]
		.filter((id) => id !== null)
		.join(' ')
	return (
		<div className="flex flex-col gap-1.5">
			<Label htmlFor={`provider-${field}`}>{label}</Label>
			<Input
				id={`provider-${field}`}
				value={values[field]}
				placeholder={placeholder}
				inputMode={inputMode}
				aria-invalid={error !== undefined}
				aria-describedby={describedBy === '' ? undefined : describedBy}
				onChange={(event) => onChange(field, event.target.value)}
			/>
			{help ? (
				<p
					id={`provider-${field}-help`}
					className="text-xs text-muted-foreground"
				>
					{help}
				</p>
			) : null}
			{error ? (
				<p
					id={`provider-${field}-error`}
					role="alert"
					className="text-sm text-destructive"
				>
					{error}
				</p>
			) : null}
		</div>
	)
}

export function ProviderForm({
	title,
	defaultConcurrency,
	editing,
	hasApiKey,
	values,
	fieldErrors,
	formError,
	keyMode,
	saving,
	dirty,
	onChange,
	onKeyModeChange,
	onSave,
	onBack,
}: ProviderFormProps) {
	const showKeyInput = !editing || keyMode === 'replace'
	const keyError = fieldErrors.api_key
	const common = { values, errors: fieldErrors, onChange }
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					Retour aux fournisseurs
				</Button>
			</div>
			<h1 className="text-2xl font-bold tracking-tight">{title}</h1>
			<form
				className="flex max-w-2xl flex-col gap-4"
				autoComplete="off"
				onSubmit={(event) => {
					event.preventDefault()
					onSave()
				}}
			>
				<TextField field="name" label="Nom" {...common} />
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="provider-kind">Type</Label>
					<select
						id="provider-kind"
						className={SELECT_CLASS}
						value={values.kind}
						aria-invalid={fieldErrors.kind !== undefined}
						onChange={(event) => onChange('kind', event.target.value)}
					>
						{PROVIDER_KINDS.map((kind) => (
							<option key={kind} value={kind}>
								{providerKindLabel(kind)}
							</option>
						))}
					</select>
					{fieldErrors.kind ? (
						<p role="alert" className="text-sm text-destructive">
							{fieldErrors.kind}
						</p>
					) : null}
				</div>
				<TextField
					field="base_url"
					label="URL de base"
					placeholder="https://api.example.com"
					{...common}
				/>
				<TextField field="model" label="Modèle" {...common} />
				<div className="grid gap-4 md:grid-cols-2">
					<TextField
						field="max_retries"
						label="Nombre max. de tentatives"
						placeholder="2"
						inputMode="numeric"
						{...common}
					/>
					<TextField
						field="max_concurrency"
						label="Requêtes simultanées"
						placeholder={String(defaultConcurrency)}
						inputMode="numeric"
						help={CONCURRENCY_HELP}
						{...common}
					/>
					<TextField
						field="timeout_seconds"
						label="Délai d'expiration (secondes)"
						placeholder="30"
						inputMode="numeric"
						{...common}
					/>
					<TextField
						field="input_cents_per_mtok"
						label="Prix en entrée (centimes par Mtok)"
						placeholder="0"
						inputMode="numeric"
						{...common}
					/>
					<TextField
						field="output_cents_per_mtok"
						label="Prix en sortie (centimes par Mtok)"
						placeholder="0"
						inputMode="numeric"
						{...common}
					/>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor="provider-reasoning_effort">Raisonnement</Label>
					<select
						id="provider-reasoning_effort"
						className={SELECT_CLASS}
						value={values.reasoning_effort}
						aria-invalid={fieldErrors.reasoning_effort !== undefined}
						aria-describedby="provider-reasoning_effort-help"
						onChange={(event) =>
							onChange('reasoning_effort', event.target.value)
						}
					>
						<option value="">Par défaut (non envoyé)</option>
						{REASONING_EFFORTS.map((effort) => (
							<option key={effort} value={effort}>
								{reasoningLabel(effort)}
							</option>
						))}
					</select>
					<p
						id="provider-reasoning_effort-help"
						className="text-xs text-muted-foreground"
					>
						{REASONING_HELP}
					</p>
					{fieldErrors.reasoning_effort ? (
						<p role="alert" className="text-sm text-destructive">
							{fieldErrors.reasoning_effort}
						</p>
					) : null}
				</div>
				<fieldset className="flex flex-col gap-3 rounded-md border p-3">
					<legend className="px-1 text-sm font-medium">
						Paramètres de génération
					</legend>
					<p className="text-xs text-muted-foreground">{GENERATION_INTRO}</p>
					<div className="grid gap-4 md:grid-cols-2">
						{GENERATION_FIELDS.map((item) => (
							<TextField key={item.field} {...item} {...common} />
						))}
					</div>
				</fieldset>
				<fieldset className="flex flex-col gap-2 rounded-md border p-3">
					<legend className="px-1 text-sm font-medium">Clé API</legend>
					{editing ? (
						<>
							<p className="text-xs text-muted-foreground">
								{hasApiKey
									? "Une clé est définie. Elle n'est jamais affichée."
									: 'Aucune clé définie.'}
							</p>
							<div className="flex flex-wrap gap-4">
								{KEY_MODES.map((item) => (
									<label
										key={item.mode}
										className="flex items-center gap-2 text-sm"
									>
										<input
											type="radio"
											name="key-mode"
											checked={keyMode === item.mode}
											onChange={() => onKeyModeChange(item.mode)}
										/>
										{item.label}
									</label>
								))}
							</div>
						</>
					) : null}
					{showKeyInput ? (
						<div className="flex flex-col gap-1.5">
							<Label htmlFor="provider-api_key">
								{editing ? 'Nouvelle clé API' : 'Clé API (facultative)'}
							</Label>
							<Input
								id="provider-api_key"
								type="password"
								autoComplete="new-password"
								value={values.api_key}
								aria-invalid={keyError !== undefined}
								onChange={(event) => onChange('api_key', event.target.value)}
							/>
						</div>
					) : null}
					{keyError ? (
						<p role="alert" className="text-sm text-destructive">
							{keyError}
						</p>
					) : null}
				</fieldset>
				{formError ? (
					<p role="alert" className="text-sm text-destructive">
						{formError}
					</p>
				) : null}
				<div className="flex items-center gap-3">
					<Button type="submit" disabled={saving || !dirty}>
						{saving
							? 'Enregistrement…'
							: editing
								? 'Enregistrer les modifications'
								: 'Créer le fournisseur'}
					</Button>
				</div>
			</form>
		</div>
	)
}
