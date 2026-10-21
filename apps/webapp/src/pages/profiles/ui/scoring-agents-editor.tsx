import { PlusIcon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import type { JsonValue, Problem } from '#/lib/schema-form'
import {
	type CalibrationStates,
	calibrationKey,
} from '#/pages/profiles/lib/agent-calibration'
import { agentField } from '#/pages/profiles/lib/agents'
import type { ProviderOption } from '#/pages/profiles/lib/qualification'
import type { ScoringActions } from '#/pages/profiles/lib/qualification-actions'
import { weightsApply } from '#/pages/profiles/lib/scoring-agents'
import { ScoringAgentCard } from '#/pages/profiles/ui/scoring-agent-card'
import { ScoringAgentSheet } from '#/pages/profiles/ui/scoring-agent-sheet'

interface ScoringAgentsEditorProps {
	agents: readonly JsonValue[]
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	editing: number | null
	confirmingRemove: number | null
	calibration: CalibrationStates
	previewing: boolean
	actions: ScoringActions
}

export function ScoringAgentsEditor({
	agents,
	problems,
	providers,
	editing,
	confirmingRemove,
	calibration,
	previewing,
	actions,
}: ScoringAgentsEditorProps) {
	const editingAgent = editing === null ? undefined : agents[editing]
	const showWeight = weightsApply(agents.length)
	return (
		<section aria-label="Agents de scoring" className="flex flex-col gap-3">
			<h2 className="text-base font-semibold">Agents de scoring</h2>
			<p className="text-sm text-muted-foreground">
				Chaque agent décide d'un score de 0 à 100 et d'un verdict pour chaque
				lead. Le score du lead est la moyenne pondérée de leurs scores, son
				verdict le plus probable de leurs verdicts.
			</p>
			<FieldErrors
				id="scoring-agents"
				problems={problems.filter(
					(problem) => problem.path === 'scoring_agents',
				)}
			/>
			{agents.length === 0 ? (
				<p className="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
					Aucun agent de scoring pour le moment : les leads restent en attente
					du scoring. Ajouter un agent pour décider du score et du verdict.
				</p>
			) : null}
			<ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
				{Object.keys(agents).map((slot) => {
					const index = Number(slot)
					return (
						<ScoringAgentCard
							key={slot}
							agent={agents[index] ?? null}
							index={index}
							problems={problems}
							providers={providers}
							showWeight={showWeight}
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
				<li data-add-scoring-agent>
					<button
						type="button"
						className="flex h-full min-h-32 w-full flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed text-sm text-muted-foreground hover:border-primary hover:text-foreground"
						onClick={actions.add}
					>
						<PlusIcon className="size-5" />
						Ajouter un agent de scoring
					</button>
				</li>
			</ul>
			{editingAgent !== undefined && editing !== null ? (
				<ScoringAgentSheet
					key={editing}
					agent={editingAgent}
					index={editing}
					problems={problems}
					providers={providers}
					showWeight={showWeight}
					calibration={
						calibration[
							calibrationKey('scoring', agentField(editingAgent, 'id'))
						]
					}
					previewing={previewing}
					actions={actions}
				/>
			) : null}
		</section>
	)
}
