import { useState } from 'react'

import type { JsonObject } from '#/lib/schema-form'
import {
	addOption,
	addQuestion,
	allAgentIds,
	newQualificationAgent,
	qualificationList,
	removeOption,
	removeQuestion,
	renameQualificationAgent,
	setAgentField,
	setAggregatorField,
	setEscalationField,
	setEvidenceField,
	setEvidenceWeight,
	setHalfLife,
	setOptionField,
	setQuestionField,
	setQuestionWording,
	setVerdictField,
	withQualification,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'

function toFraction(percent: number | null): number | null {
	return percent === null ? null : percent / 100
}

export function useQualificationUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
) {
	const [editing, setEditing] = useState<number | null>(null)
	const [confirmingRemove, setConfirmingRemove] = useState<number | null>(null)
	const [picking, setPicking] = useState(false)
	const [previewing, setPreviewing] = useState(false)
	const agents = qualificationList(value)

	const actions: QualificationActions = {
		openPicker: () => setPicking(true),
		closePicker: () => setPicking(false),
		add: (templateId) => {
			onChange(
				withQualification(value, [
					...agents,
					newQualificationAgent(templateId, allAgentIds(value)),
				]),
			)
			setPicking(false)
			setEditing(agents.length)
		},
		edit: setEditing,
		closeEdit: () => {
			setEditing(null)
			setPreviewing(false)
		},
		setEnabled: (index, enabled) =>
			onChange(setAgentField(value, index, 'enabled', enabled)),
		setName: (index, name) =>
			onChange(renameQualificationAgent(value, index, name)),
		setDescription: (index, description) =>
			onChange(setAgentField(value, index, 'description', description)),
		setId: (index, id) => onChange(setAgentField(value, index, 'id', id)),
		setProvider: (index, providerId) =>
			onChange(
				setAgentField(
					value,
					index,
					'provider_id',
					providerId === '' ? null : providerId,
				),
			),
		setWeight: (index, weight) =>
			onChange(setAgentField(value, index, 'weight', weight)),
		setExclusionThreshold: (index, percent) =>
			onChange(
				setAgentField(value, index, 'exclusion_threshold', toFraction(percent)),
			),
		setSamples: (index, samples) =>
			onChange(setAgentField(value, index, 'samples', samples)),
		setSamplingTemperature: (index, temperature) =>
			onChange(
				setAgentField(value, index, 'sampling_temperature', temperature),
			),
		setPrompt: (index, prompt) =>
			onChange(setAgentField(value, index, 'system_prompt', prompt)),
		requestRemove: setConfirmingRemove,
		cancelRemove: () => setConfirmingRemove(null),
		confirmRemove: (index) => {
			onChange(
				withQualification(
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
		addQuestion: (index) => onChange(addQuestion(value, index)),
		removeQuestion: (index, question) =>
			onChange(removeQuestion(value, index, question)),
		setQuestionWording: (index, question, text) =>
			onChange(setQuestionWording(value, index, question, text)),
		setQuestionId: (index, question, id) =>
			onChange(setQuestionField(value, index, question, 'id', id)),
		setQuestionWeight: (index, question, weight) =>
			onChange(setQuestionField(value, index, question, 'weight', weight)),
		addOption: (index, question) => onChange(addOption(value, index, question)),
		removeOption: (index, question, option) =>
			onChange(removeOption(value, index, question, option)),
		setOptionValue: (index, question, option, text) =>
			onChange(setOptionField(value, index, question, option, 'value', text)),
		setOptionLabel: (index, question, option, text) =>
			onChange(setOptionField(value, index, question, option, 'label', text)),
		setOptionScore: (index, question, option, score) =>
			onChange(setOptionField(value, index, question, option, 'score', score)),
		setOptionExcludes: (index, question, option, excludes) =>
			onChange(
				setOptionField(value, index, question, option, 'excludes', excludes),
			),
		setEscalationProvider: (providerId) =>
			onChange(
				setEscalationField(
					value,
					'provider',
					providerId === '' ? null : providerId,
				),
			),
		setBandLow: (percent) =>
			onChange(setEscalationField(value, 'band_low', percent)),
		setBandHigh: (percent) =>
			onChange(setEscalationField(value, 'band_high', percent)),
		setAggregatorProvider: (providerId) =>
			onChange(
				setAggregatorField(
					value,
					'provider_id',
					providerId === '' ? null : providerId,
				),
			),
		setAggregatorPrompt: (prompt) =>
			onChange(setAggregatorField(value, 'system_prompt', prompt)),
		setReliableFrom: (percent) =>
			onChange(setVerdictField(value, 'reliable_from', percent)),
		setDiscardBelow: (percent) =>
			onChange(setVerdictField(value, 'discard_below', percent)),
		setCalibrationFloor: (percent) =>
			onChange(
				setVerdictField(value, 'calibration_floor', toFraction(percent)),
			),
		setMinEvidence: (percent) =>
			onChange(setVerdictField(value, 'min_evidence', toFraction(percent))),
		setInfluence: (percent) =>
			onChange(setEvidenceField(value, 'influence', toFraction(percent))),
		setReferenceScore: (score) =>
			onChange(setEvidenceField(value, 'reference_score', score)),
		setEvidenceWeight: (key, weight) =>
			onChange(setEvidenceWeight(value, key, weight)),
		setHalfLife: (key, days) => onChange(setHalfLife(value, key, days)),
	}

	return { agents, editing, confirmingRemove, picking, previewing, actions }
}
