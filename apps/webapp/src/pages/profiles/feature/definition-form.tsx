import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import {
	fieldPaths,
	isJsonObject,
	type JsonObject,
	resolve,
} from '#/lib/schema-form'
import { useAgentsUi } from '#/pages/profiles/feature/use-agents-ui'
import { useScoringUi } from '#/pages/profiles/feature/use-scoring-ui'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import type { EditorTab } from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { AgentsEditor } from '#/pages/profiles/ui/agents-editor'
import { ScoringGrid } from '#/pages/profiles/ui/scoring-grid'

export function unmatchedProblems(
	value: JsonObject,
	problems: readonly Problem[],
): Problem[] {
	const known = fieldPaths(PROFILE_SCHEMA, PROFILE_SCHEMA, value)
	return problems.filter((problem) => !known.has(problem.path))
}

interface DefinitionTabContentProps {
	tab: EditorTab
	value: JsonObject
	problems: readonly Problem[]
	onChange: (next: JsonObject) => void
}

function section(key: string) {
	return resolve(PROFILE_SCHEMA, PROFILE_SCHEMA.properties?.[key] ?? {})
}

function sectionValue(value: JsonObject, key: string) {
	return value[key]
}

export function DefinitionTabContent({
	tab,
	value,
	problems,
	onChange,
}: DefinitionTabContentProps) {
	const agents = useAgentsUi(value, onChange)
	const scoring = useScoringUi(value, onChange)
	const context: FormContext = {
		root: PROFILE_SCHEMA,
		hints: PROFILE_HINTS,
		value,
		problems,
		onChange: (next) => {
			if (isJsonObject(next)) onChange(next)
		},
	}
	const fields = (key: string) => (
		<ObjectFields
			context={context}
			schema={section(key)}
			path={[key]}
			value={sectionValue(value, key)}
		/>
	)

	switch (tab) {
		case 'identity':
			return fields('identity')
		case 'scoring':
			return (
				<ScoringGrid
					factors={scoring.factors}
					problems={problems}
					suggestions={scoring.suggestions}
					available={scoring.available}
					maxWeight={scoring.maxWeight}
					onAdd={scoring.add}
					onRemove={scoring.remove}
					onSignal={scoring.setSignal}
					onWeight={scoring.setWeight}
					onValue={scoring.setValue}
				/>
			)
		case 'settings':
			return (
				<div className="flex flex-col gap-6">
					<section className="flex flex-col gap-3">
						<h2 className="text-base font-semibold">LLM</h2>
						{fields('llm')}
					</section>
					<section className="flex flex-col gap-3">
						<h2 className="text-base font-semibold">Contact ranking</h2>
						{fields('contact_ranking')}
					</section>
				</div>
			)
		case 'agents':
			return (
				<AgentsEditor
					context={context}
					agents={agents.agents}
					problems={problems}
					collapsed={agents.collapsed}
					confirmingRemove={agents.confirmingRemove}
					picking={agents.picking}
					onOpenPicker={agents.openPicker}
					onClosePicker={agents.closePicker}
					onAdd={agents.add}
					onToggleCollapsed={agents.toggleCollapsed}
					onEnabled={agents.setEnabled}
					onName={agents.setName}
					onId={agents.setId}
					onCriteria={agents.setCriteria}
					onRequestRemove={agents.requestRemove}
					onConfirmRemove={agents.confirmRemove}
					onCancelRemove={agents.cancelRemove}
				/>
			)
	}
}
