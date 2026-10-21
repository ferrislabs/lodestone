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
import { useAgentWizard } from '#/pages/profiles/feature/use-agent-wizard'
import { useAgentsUi } from '#/pages/profiles/feature/use-agents-ui'
import type { EcosystemUi } from '#/pages/profiles/feature/use-ecosystem-ui'
import { useQualificationUi } from '#/pages/profiles/feature/use-qualification-ui'
import { useScoringAgentsUi } from '#/pages/profiles/feature/use-scoring-agents-ui'
import {
	type CalibrationStates,
	NO_CALIBRATION_STATES,
} from '#/pages/profiles/lib/agent-calibration'
import { dependencyNote } from '#/pages/profiles/lib/agents'
import {
	advancedHasProblem,
	customSection,
	enrichmentSection,
	inspectionEnabled,
	moduleCards,
	searchKeywordUnion,
	withInspectionEnabled,
} from '#/pages/profiles/lib/enrichment'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import type { ProviderOption } from '#/pages/profiles/lib/qualification'
import { weightsApply } from '#/pages/profiles/lib/scoring-agents'
import type { EditorTab } from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { AgentPage } from '#/pages/profiles/ui/agent-page'
import { AgentWizard } from '#/pages/profiles/ui/agent-wizard'
import { AgentsEditor } from '#/pages/profiles/ui/agents-editor'
import { AgentsHelp } from '#/pages/profiles/ui/agents-help'
import { EnrichmentEditor } from '#/pages/profiles/ui/enrichment-editor'
import { EnrichmentHelp } from '#/pages/profiles/ui/enrichment-help'
import { QualificationEditor } from '#/pages/profiles/ui/qualification-editor'
import { QualificationHelp } from '#/pages/profiles/ui/qualification-help'
import { ScoringAgentsEditor } from '#/pages/profiles/ui/scoring-agents-editor'
import { ScoringHelp } from '#/pages/profiles/ui/scoring-help'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
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
	catalog?: readonly ProviderSummary[]
	onOpenProvider?: (key: string) => void
	calibration?: CalibrationStates
	agentId?: string | null
	onAgentChange?: (id: string | null, replace: boolean) => void
	ecosystem?: EcosystemUi
}

const NO_PROVIDERS: readonly ProviderOption[] = []
const NO_CATALOG: readonly ProviderSummary[] = []
const NO_OPEN = () => undefined
const NO_ECOSYSTEM: EcosystemUi = {
	modules: [],
	modulesState: 'ready',
	test: null,
	onTest: NO_OPEN,
	testActions: {
		onClose: NO_OPEN,
		onQuery: NO_OPEN,
		onPick: NO_OPEN,
		onRun: NO_OPEN,
	},
}

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
	catalog = NO_CATALOG,
	onOpenProvider = NO_OPEN,
	calibration = NO_CALIBRATION_STATES,
	agentId,
	onAgentChange,
	ecosystem = NO_ECOSYSTEM,
}: DefinitionTabContentProps) {
	const [localAgent, setLocalAgent] = useState<string | null>(null)
	if (tab !== 'sourcing' && localAgent !== null) setLocalAgent(null)
	const routed = onAgentChange !== undefined
	const openAgent = (id: string | null, replace: boolean) => {
		if (onAgentChange) onAgentChange(id, replace)
		else setLocalAgent(id)
	}
	const agents = useAgentsUi(
		value,
		onChange,
		routed ? (agentId ?? null) : localAgent,
		openAgent,
	)
	const wizard = useAgentWizard(value, onChange, agents.setHighlighted)
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
		case 'enrichment':
			return (
				<div
					data-testid="help-layout"
					className="flex flex-col gap-4 lg:grid lg:grid-cols-4 lg:gap-6"
				>
					<EnrichmentHelp
						collapsed={helpCollapsed}
						onToggle={() => setHelpCollapsed((current) => !current)}
					/>
					<div className="lg:order-1 lg:col-span-3">
						<EnrichmentEditor
							context={context}
							enrichment={enrichmentSection(value)}
							custom={customSection(value)}
							enabled={inspectionEnabled(value)}
							cards={moduleCards(ecosystem.modules, inspectionEnabled(value))}
							modulesState={ecosystem.modulesState}
							keywordUnion={searchKeywordUnion(value)}
							advancedOpen={advancedHasProblem(problems)}
							test={ecosystem.test}
							onEnabled={(enabled) =>
								onChange(withInspectionEnabled(value, enabled))
							}
							onTest={ecosystem.onTest}
							testActions={ecosystem.testActions}
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
		case 'sourcing': {
			const index = agents.openIndex
			const open = index >= 0 ? agents.agents[index] : undefined
			if (open !== undefined) {
				return (
					<AgentPage
						key={index}
						context={context}
						agent={open}
						index={index}
						problems={problems}
						note={dependencyNote(open, agents.agents)}
						catalog={catalog}
						countryQuery={agents.countryQuery}
						confirmingRemove={agents.confirmingRemove === index}
						actions={{
							onBack: agents.back,
							onName: (name) => agents.setName(index, name),
							onId: (id) => agents.setId(index, id),
							onEnabled: (enabled) => agents.setEnabled(index, enabled),
							onProvider: (key, attached) =>
								agents.setProvider(index, key, attached),
							onOpenProvider,
							onRemote: (remote) => agents.setRemote(index, remote),
							onContract: (contract, selected) =>
								agents.setContract(index, contract, selected),
							onCountries: (codes) => agents.setCountries(index, codes),
							onCountryQuery: agents.setCountryQuery,
							onRequestRemove: () => agents.requestRemove(index),
							onConfirmRemove: () => agents.confirmRemove(index),
							onCancelRemove: agents.cancelRemove,
						}}
					/>
				)
			}
			const wizardState = wizard.state
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
							agents={agents.agents}
							problems={problems}
							highlighted={agents.highlighted}
							confirmingRemove={agents.confirmingRemove}
							catalog={catalog}
							sourceTrigger={sourceTrigger}
							listeners={listeners}
							now={now}
							onAdd={wizard.open}
							onEdit={agents.edit}
							onEnabled={agents.setEnabled}
							onRequestRemove={agents.requestRemove}
							onConfirmRemove={agents.confirmRemove}
							onCancelRemove={agents.cancelRemove}
						/>
						{wizardState !== null ? (
							<AgentWizard
								step={wizardState.step}
								kind={wizardState.kind}
								agent={wizardState.agent}
								index={wizard.index}
								context={{
									...context,
									value: wizard.preview,
									problems: [],
									onChange: wizard.setPreview,
								}}
								catalog={catalog}
								countryQuery={wizardState.countryQuery}
								problems={wizard.problems}
								actions={{
									onKind: wizard.setKind,
									onProvider: wizard.setProvider,
									onOpenProvider,
									onRemote: wizard.setRemote,
									onContract: wizard.setContract,
									onCountries: wizard.setCountries,
									onCountryQuery: wizard.setCountryQuery,
									onName: wizard.setName,
									onNext: wizard.next,
									onBack: wizard.back,
									onCreate: wizard.create,
									onClose: wizard.close,
								}}
							/>
						) : null}
						<section className="flex flex-col gap-3">
							<h2 className="text-base font-semibold">Contacts</h2>
							{fields('contact_ranking')}
						</section>
					</div>
				</div>
			)
		}
	}
}
