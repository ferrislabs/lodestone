import { PlusIcon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import type { JsonObject, JsonValue, Problem } from '#/lib/schema-form'
import {
	type CalibrationStates,
	calibrationKey,
} from '#/pages/profiles/lib/agent-calibration'
import { agentField } from '#/pages/profiles/lib/agents'
import {
	escalationOf,
	evidenceSettingsOf,
	type ProviderOption,
	verdictOf,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { AggregatorPanel } from '#/pages/profiles/ui/aggregator-panel'
import { EscalationPanel } from '#/pages/profiles/ui/escalation-panel'
import { QualificationCard } from '#/pages/profiles/ui/qualification-card'
import { QualificationPicker } from '#/pages/profiles/ui/qualification-picker'
import { QualificationSheet } from '#/pages/profiles/ui/qualification-sheet'
import { VerdictPanel } from '#/pages/profiles/ui/verdict-panel'

interface QualificationEditorProps {
	value: JsonObject
	agents: readonly JsonValue[]
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	editing: number | null
	confirmingRemove: number | null
	picking: boolean
	calibration: CalibrationStates
	previewing: boolean
	actions: QualificationActions
}

export function QualificationEditor({
	value,
	agents,
	problems,
	providers,
	editing,
	confirmingRemove,
	picking,
	calibration,
	previewing,
	actions,
}: QualificationEditorProps) {
	const editingAgent = editing === null ? undefined : agents[editing]
	return (
		<div className="flex flex-col gap-6">
			<section
				aria-label="Agents de qualification"
				className="flex flex-col gap-3"
			>
				<h2 className="text-base font-semibold">Agents de qualification</h2>
				<FieldErrors
					id="qualification-agents"
					problems={problems.filter(
						(problem) => problem.path === 'qualification_agents',
					)}
				/>
				{agents.length === 0 ? (
					<p className="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
						Aucun agent de qualification pour le moment. Ajouter un agent pour
						évaluer les leads.
					</p>
				) : null}
				<ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
					{Object.keys(agents).map((slot) => {
						const index = Number(slot)
						return (
							<QualificationCard
								key={slot}
								agent={agents[index] ?? null}
								index={index}
								problems={problems}
								providers={providers}
								editing={editing === index}
								confirmingRemove={confirmingRemove === index}
								onEdit={() => actions.edit(index)}
								onEnabled={(enabled) => actions.setEnabled(index, enabled)}
								onRequestRemove={() => actions.requestRemove(index)}
								onConfirmRemove={() => actions.confirmRemove(index)}
								onCancelRemove={actions.cancelRemove}
							/>
						)
					})}
					<li data-add-qualification-agent>
						<button
							type="button"
							className="flex h-full min-h-32 w-full flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed text-sm text-muted-foreground hover:border-primary hover:text-foreground"
							onClick={actions.openPicker}
						>
							<PlusIcon className="size-5" />
							Ajouter un agent de qualification
						</button>
					</li>
				</ul>
				{picking ? (
					<QualificationPicker
						onPick={actions.add}
						onCancel={actions.closePicker}
					/>
				) : null}
			</section>
			<AggregatorPanel
				value={value}
				problems={problems}
				providers={providers}
				actions={actions}
			/>
			<EscalationPanel
				escalation={escalationOf(value)}
				problems={problems}
				providers={providers}
				actions={actions}
			/>
			<VerdictPanel
				verdict={verdictOf(value)}
				evidence={evidenceSettingsOf(value)}
				problems={problems}
				actions={actions}
			/>
			{editingAgent !== undefined && editing !== null ? (
				<QualificationSheet
					key={editing}
					agent={editingAgent}
					index={editing}
					problems={problems}
					providers={providers}
					calibration={
						calibration[
							calibrationKey('qualification', agentField(editingAgent, 'id'))
						]
					}
					previewing={previewing}
					actions={actions}
				/>
			) : null}
		</div>
	)
}
