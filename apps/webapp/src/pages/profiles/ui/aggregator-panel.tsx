import { FieldErrors } from '#/components/schema-form/field-frame'
import { Label } from '#/components/ui/label'
import { Textarea } from '#/components/ui/textarea'
import type { JsonObject, Problem } from '#/lib/schema-form'
import { agentField } from '#/pages/profiles/lib/agents'
import {
	agentProvider,
	aggregatorOf,
	MAX_PROMPT_CHARS,
	type ProviderOption,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { ProviderSelect } from '#/pages/profiles/ui/provider-select'

interface AggregatorPanelProps {
	value: JsonObject
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	actions: QualificationActions
}

function at(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

export function AggregatorPanel({
	value,
	problems,
	providers,
	actions,
}: AggregatorPanelProps) {
	const aggregator = aggregatorOf(value)
	const prompt = agentField(aggregator, 'system_prompt')
	const providerProblems = at(problems, 'aggregator.provider_id')
	const promptProblems = at(problems, 'aggregator.system_prompt')
	return (
		<section
			aria-label="Agrégateur"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-sm"
		>
			<div className="flex flex-col gap-1">
				<h2 className="text-base font-semibold">Agrégateur</h2>
				<p className="text-xs text-muted-foreground">
					Rédige la synthèse finale et le brouillon du lead à partir des
					rapports de tous les agents de qualification. Il ne change ni la
					confiance ni le verdict.
				</p>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="aggregator-provider">Fournisseur LLM</Label>
				<ProviderSelect
					id="aggregator-provider"
					value={agentProvider(aggregator)}
					providers={providers}
					invalid={providerProblems.length > 0}
					onChange={actions.setAggregatorProvider}
				/>
				<FieldErrors id="aggregator-provider" problems={providerProblems} />
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="aggregator-prompt">Prompt système</Label>
				<Textarea
					id="aggregator-prompt"
					rows={8}
					value={prompt}
					aria-invalid={promptProblems.length > 0}
					onChange={(event) => actions.setAggregatorPrompt(event.target.value)}
				/>
				<p className="text-xs text-muted-foreground">
					{prompt.length} / {MAX_PROMPT_CHARS} caractères
				</p>
				<FieldErrors id="aggregator-prompt" problems={promptProblems} />
			</div>
		</section>
	)
}
