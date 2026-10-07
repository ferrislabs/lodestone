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
import { useQualificationUi } from '#/pages/profiles/feature/use-qualification-ui'
import { useScoringAgentsUi } from '#/pages/profiles/feature/use-scoring-agents-ui'
import { useThresholdUi } from '#/pages/profiles/feature/use-threshold-ui'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import type { ProviderOption } from '#/pages/profiles/lib/qualification'
import type { EditorTab } from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { AgentsEditor } from '#/pages/profiles/ui/agents-editor'
import { AgentsHelp } from '#/pages/profiles/ui/agents-help'
import { QualificationEditor } from '#/pages/profiles/ui/qualification-editor'
import { QualificationHelp } from '#/pages/profiles/ui/qualification-help'
import { ScoringAgentsEditor } from '#/pages/profiles/ui/scoring-agents-editor'
import { ScoringHelp } from '#/pages/profiles/ui/scoring-help'
import { ThresholdField } from '#/pages/profiles/ui/threshold-field'
import type { SourceTrigger } from '#/pages/sources/feature/use-source-trigger'
import type { ListenerSummary } from '#/pages/sources/lib/types'

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
	sourceTrigger?: SourceTrigger
	listeners?: Readonly<Record<string, ListenerSummary>>
	now?: number
	providers?: readonly ProviderOption[]
}

const NO_PROVIDERS: readonly ProviderOption[] = []

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
	sourceTrigger,
	listeners,
	now,
	providers = NO_PROVIDERS,
}: DefinitionTabContentProps) {
	const agents = useAgentsUi(value, onChange)
	const qualification = useQualificationUi(value, onChange)
	const threshold = useThresholdUi(value, onChange)
	const scoringAgents = useScoringAgentsUi(value, onChange)
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
					<ScoringHelp
						collapsed={helpCollapsed}
						onToggle={() => setHelpCollapsed((current) => !current)}
					/>
					<div className="flex flex-col gap-8 lg:order-1 lg:col-span-3">
						<ScoringAgentsEditor
							agents={scoringAgents.agents}
							problems={problems}
							providers={providers}
							editing={scoringAgents.editing}
							confirmingRemove={scoringAgents.confirmingRemove}
							previewing={scoringAgents.previewing}
							actions={scoringAgents.actions}
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
							value={threshold.value}
							problems={problems}
							onChange={threshold.set}
						/>
						{fields('llm')}
					</section>
					<section className="flex flex-col gap-3">
						<h2 className="text-base font-semibold">Calibration</h2>
						{fields('calibration')}
					</section>
					<section className="flex flex-col gap-3">
						<h2 className="text-base font-semibold">Contact ranking</h2>
						{fields('contact_ranking')}
					</section>
				</div>
			)
		case 'qualification':
			return (
				<div
					data-testid="help-layout"
					className="flex flex-col gap-4 lg:grid lg:grid-cols-4 lg:gap-6"
				>
					<QualificationHelp
						collapsed={helpCollapsed}
						onToggle={() => setHelpCollapsed((current) => !current)}
					/>
					<div className="lg:order-1 lg:col-span-3">
						<QualificationEditor
							value={value}
							agents={qualification.agents}
							problems={problems}
							providers={providers}
							editing={qualification.editing}
							confirmingRemove={qualification.confirmingRemove}
							picking={qualification.picking}
							previewing={qualification.previewing}
							actions={qualification.actions}
						/>
					</div>
				</div>
			)
		case 'sourcing':
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
							sourceTrigger={sourceTrigger}
							listeners={listeners}
							now={now}
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
