import { useState } from 'react'

import type { JsonObject } from '#/lib/schema-form'
import {
	addOption,
	addQuestion,
	allAgentIds,
	removeOption,
	removeQuestion,
	SCORING_SCOPE,
	scoringList,
	setOptionField,
	setQuestionField,
	setQuestionWording,
	withScoring,
} from '#/pages/profiles/lib/qualification'
import type { ScoringActions } from '#/pages/profiles/lib/qualification-actions'
import {
	newScoringAgent,
	renameScoringAgent,
	setScoringField,
} from '#/pages/profiles/lib/scoring-agents'

export function useScoringAgentsUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
) {
	const [editing, setEditing] = useState<number | null>(null)
	const [confirmingRemove, setConfirmingRemove] = useState<number | null>(null)
	const [previewing, setPreviewing] = useState(false)
	const agents = scoringList(value)

	const actions: ScoringActions = {
		add: () => {
			onChange(
				withScoring(value, [
					...agents,
					newScoringAgent(allAgentIds(value, null, SCORING_SCOPE)),
				]),
			)
			setEditing(agents.length)
		},
		edit: setEditing,
		closeEdit: () => {
			setEditing(null)
			setPreviewing(false)
		},
		setEnabled: (index, enabled) =>
			onChange(setScoringField(value, index, 'enabled', enabled)),
		setName: (index, name) => onChange(renameScoringAgent(value, index, name)),
		setDescription: (index, description) =>
			onChange(setScoringField(value, index, 'description', description)),
		setId: (index, id) => onChange(setScoringField(value, index, 'id', id)),
		setProvider: (index, providerId) =>
			onChange(
				setScoringField(
					value,
					index,
					'provider_id',
					providerId === '' ? null : providerId,
				),
			),
		setWeight: (index, weight) =>
			onChange(setScoringField(value, index, 'weight', weight)),
		setSamples: (index, samples) =>
			onChange(setScoringField(value, index, 'samples', samples)),
		setSamplingTemperature: (index, temperature) =>
			onChange(
				setScoringField(value, index, 'sampling_temperature', temperature),
			),
		setPrompt: (index, prompt) =>
			onChange(setScoringField(value, index, 'system_prompt', prompt)),
		requestRemove: setConfirmingRemove,
		cancelRemove: () => setConfirmingRemove(null),
		confirmRemove: (index) => {
			onChange(
				withScoring(
					value,
					agents.filter((_, position) => position !== index),
				),
			)
			setConfirmingRemove(null)
			setEditing((current) => {
				if (current === null || current === index) return null
				return current > index ? current - 1 : current
			})
		},
		togglePreview: () => setPreviewing((current) => !current),
		addCriterion: (index) => onChange(addQuestion(value, index, SCORING_SCOPE)),
		removeQuestion: (index, question) =>
			onChange(removeQuestion(value, index, question, SCORING_SCOPE)),
		setQuestionWording: (index, question, text) =>
			onChange(setQuestionWording(value, index, question, text, SCORING_SCOPE)),
		setQuestionId: (index, question, id) =>
			onChange(
				setQuestionField(value, index, question, 'id', id, SCORING_SCOPE),
			),
		setQuestionWeight: (index, question, weight) =>
			onChange(
				setQuestionField(
					value,
					index,
					question,
					'weight',
					weight,
					SCORING_SCOPE,
				),
			),
		addOption: (index, question) =>
			onChange(addOption(value, index, question, SCORING_SCOPE)),
		removeOption: (index, question, option) =>
			onChange(removeOption(value, index, question, option, SCORING_SCOPE)),
		setOptionValue: (index, question, option, text) =>
			onChange(
				setOptionField(
					value,
					index,
					question,
					option,
					'value',
					text,
					SCORING_SCOPE,
				),
			),
		setOptionLabel: (index, question, option, text) =>
			onChange(
				setOptionField(
					value,
					index,
					question,
					option,
					'label',
					text,
					SCORING_SCOPE,
				),
			),
		setOptionScore: (index, question, option, score) =>
			onChange(
				setOptionField(
					value,
					index,
					question,
					option,
					'score',
					score,
					SCORING_SCOPE,
				),
			),
	}

	return { agents, editing, confirmingRemove, previewing, actions }
}
