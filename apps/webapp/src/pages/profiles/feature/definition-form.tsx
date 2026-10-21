import { useState } from 'react'

import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import {
	fieldPaths,
	isJsonObject,
	type JsonObject,
	resolve,
	type Schema,
} from '#/lib/schema-form'
import { useAgentsUi } from '#/pages/profiles/feature/use-agents-ui'
import { useQualificationUi } from '#/pages/profiles/feature/use-qualification-ui'
import { useScoringAgentsUi } from '#/pages/profiles/feature/use-scoring-agents-ui'
import {
	type CalibrationStates,
	NO_CALIBRATION_STATES,
} from '#/pages/profiles/lib/agent-calibration'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import type { ProviderOption } from '#/pages/profiles/lib/qualification'
import { weightsApply } from '#/pages/profiles/lib/scoring-agents'
import type { EditorTab } from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { AgentsEditor } from '#/pages/profiles/ui/agents-editor'
import { AgentsHelp } from '#/pages/profiles/ui/agents-help'
import { QualificationEditor } from '#/pages/profiles/ui/qualification-editor'
import { QualificationHelp } from '#/pages/profiles/ui/qualification-help'
import { ScoringAgentsEditor } from '#/pages/profiles/ui/scoring-agents-editor'
import { ScoringHelp } from '#/pages/profiles/ui/scoring-help'
import type { SourceTrigger } from '#/pages/sources/feature/use-source-trigger'
import type { ListenerSummary } from '#/pages/sources/lib/types'

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
	calibration?: CalibrationStates
}

const NO_PROVIDERS: readonly ProviderOption[] = []

function section(key: string) {
	return resolve(PROFILE_SCHEMA, PROFILE_SCHEMA.properties?.[key] ?? {})
}

const LANGUAGE_SCHEMA: Schema = {
	properties: { language: PROFILE_SCHEMA.properties?.language ?? {} },
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
	calibration = NO_CALIBRATION_STATES,
}: DefinitionTabContentProps) {
	const agents = useAgentsUi(value, onChange)
	const qualification = useQualificationUi(value, onChange)
	const scoringAgents = useScoringAgentsUi(value, onChange)
	const [helpCollapsed, setHelpCollapsed] = useState(false)
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
			return (
				<div className="flex flex-col gap-6">
					{fields('identity')}
					<ObjectFields
						context={context}
						schema={LANGUAGE_SCHEMA}
						path={[]}
						value={value}
					/>
				</div>
			)
		case 'scoring':
			return (
				<div
					data-testid="help-layout"
					className="flex flex-col gap-4 lg:grid lg:grid-cols-4 lg:gap-6"
				>
					<ScoringHelp
						collapsed={helpCollapsed}
						showWeight={weightsApply(scoringAgents.agents.length)}
						onToggle={() => setHelpCollapsed((current) => !current)}
					/>
					<div className="flex flex-col gap-8 lg:order-1 lg:col-span-3">
						<ScoringAgentsEditor
							agents={scoringAgents.agents}
							problems={problems}
							providers={providers}
							calibration={calibration}
							editing={scoringAgents.editing}
							confirmingRemove={scoringAgents.confirmingRemove}
							previewing={scoringAgents.previewing}
							actions={scoringAgents.actions}
						/>
					</div>
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
							calibration={calibration}
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
					<div className="flex flex-col gap-8 lg:order-1 lg:col-span-3">
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
							onRequestRemove={agents.requestRemove}
							onConfirmRemove={agents.confirmRemove}
							onCancelRemove={agents.cancelRemove}
						/>
						<section className="flex flex-col gap-3">
							<h2 className="text-base font-semibold">Contacts</h2>
							{fields('contact_ranking')}
						</section>
					</div>
				</div>
			)
	}
}
