import { useState } from 'react'

import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import {
	fieldPaths,
	isJsonObject,
	type JsonObject,
	resolve,
	type UiHints,
} from '#/lib/schema-form'
import { useAgentsUi } from '#/pages/profiles/feature/use-agents-ui'
import { useScoringUi } from '#/pages/profiles/feature/use-scoring-ui'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import {
	isValidThreshold,
	qualificationPoints,
} from '#/pages/profiles/lib/scoring'
import {
	thresholdSummary,
	workedExample,
} from '#/pages/profiles/lib/scoring-explain'
import type { EditorTab } from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { AgentsEditor } from '#/pages/profiles/ui/agents-editor'
import { AgentsHelp } from '#/pages/profiles/ui/agents-help'
import { ScoringExplainer } from '#/pages/profiles/ui/scoring-explainer'
import { ScoringGrid } from '#/pages/profiles/ui/scoring-grid'
import { ThresholdField } from '#/pages/profiles/ui/threshold-field'

const DEFINITION_HINTS: UiHints = {
	...PROFILE_HINTS,
	hidden: [...PROFILE_HINTS.hidden, 'llm.qualification_threshold'],
}

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
	onNavigate?: (tab: EditorTab) => void
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
	onNavigate,
}: DefinitionTabContentProps) {
	const agents = useAgentsUi(value, onChange)
	const scoring = useScoringUi(value, onChange)
	const [helpCollapsed, setHelpCollapsed] = useState(false)
	const context: FormContext = {
		root: PROFILE_SCHEMA,
		hints: DEFINITION_HINTS,
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
				<div
					data-testid="help-layout"
					className="flex flex-col gap-4 lg:grid lg:grid-cols-4 lg:gap-6"
				>
					<ScoringExplainer
						collapsed={helpCollapsed}
						onToggle={() => setHelpCollapsed((current) => !current)}
						thresholdSummary={thresholdSummary(
							scoring.threshold,
							isValidThreshold(scoring.threshold)
								? qualificationPoints(scoring.threshold, scoring.maxScore)
								: null,
						)}
						example={workedExample(scoring.factors)}
						onNavigate={() => onNavigate?.('settings')}
					/>
					<div className="lg:order-1 lg:col-span-3">
						<ScoringGrid
							factors={scoring.factors}
							problems={problems}
							suggestions={scoring.suggestions}
							available={scoring.available}
							maxScore={scoring.maxScore}
							onAdd={scoring.add}
							onRemove={scoring.remove}
							onSignal={scoring.setSignal}
							onWeight={scoring.setWeight}
							onValue={scoring.setValue}
						/>
					</div>
				</div>
			)
		case 'settings':
			return (
				<div className="flex flex-col gap-6">
					<section className="flex flex-col gap-3">
						<h2 className="text-base font-semibold">LLM</h2>
						<ThresholdField
							value={scoring.threshold}
							maxScore={scoring.maxScore}
							problems={problems}
							onChange={scoring.setThreshold}
						/>
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
				<div
					data-testid="help-layout"
					className="flex flex-col gap-4 lg:grid lg:grid-cols-4 lg:gap-6"
				>
					<AgentsHelp
						collapsed={helpCollapsed}
						onToggle={() => setHelpCollapsed((current) => !current)}
					/>
					<div className="lg:order-1 lg:col-span-3">
						<AgentsEditor
							context={context}
							agents={agents.agents}
							problems={problems}
							editing={agents.editing}
							confirmingRemove={agents.confirmingRemove}
							picking={agents.picking}
							onOpenPicker={agents.openPicker}
							onClosePicker={agents.closePicker}
							onAdd={agents.add}
							onEdit={agents.edit}
							onCloseEdit={agents.closeEdit}
							onEnabled={agents.setEnabled}
							onName={agents.setName}
							onId={agents.setId}
							onCriteria={agents.setCriteria}
							onRequestRemove={agents.requestRemove}
							onConfirmRemove={agents.confirmRemove}
							onCancelRemove={agents.cancelRemove}
						/>
					</div>
				</div>
			)
	}
}
