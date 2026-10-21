import type {
	EvidenceWeightKey,
	HalfLifeKey,
} from '#/pages/profiles/lib/qualification'

export interface AxisActions {
	removeQuestion: (index: number, question: number) => void
	setQuestionWording: (index: number, question: number, text: string) => void
	setQuestionId: (index: number, question: number, id: string) => void
	setQuestionWeight: (index: number, question: number, weight: number) => void
	addOption: (index: number, question: number) => void
	removeOption: (index: number, question: number, option: number) => void
	setOptionValue: (
		index: number,
		question: number,
		option: number,
		text: string,
	) => void
	setOptionLabel: (
		index: number,
		question: number,
		option: number,
		text: string,
	) => void
	setOptionScore: (
		index: number,
		question: number,
		option: number,
		score: number | null,
	) => void
	setOptionExcludes?: (
		index: number,
		question: number,
		option: number,
		excludes: boolean,
	) => void
}

export interface QualificationActions extends AxisActions {
	openPicker: () => void
	closePicker: () => void
	add: (templateId: string) => void
	edit: (index: number) => void
	closeEdit: () => void
	setEnabled: (index: number, enabled: boolean) => void
	setName: (index: number, name: string) => void
	setDescription: (index: number, description: string) => void
	setId: (index: number, id: string) => void
	setProvider: (index: number, providerId: string) => void
	setLearnedCalibration: (index: number, learned: boolean) => void
	setExclusionThreshold: (index: number, percent: number | null) => void
	setSamples: (index: number, samples: number | null) => void
	setSamplingTemperature: (index: number, temperature: number | null) => void
	setPrompt: (index: number, prompt: string) => void
	requestRemove: (index: number) => void
	confirmRemove: (index: number) => void
	cancelRemove: () => void
	togglePreview: () => void
	addQuestion: (index: number) => void
	setEscalationProvider: (providerId: string) => void
	setBandLow: (percent: number | null) => void
	setBandHigh: (percent: number | null) => void
	setAggregatorProvider: (providerId: string) => void
	setAggregatorPrompt: (prompt: string) => void
	setReliableFrom: (percent: number | null) => void
	setDiscardBelow: (percent: number | null) => void
	setCalibrationFloor: (percent: number | null) => void
	setMinEvidence: (percent: number | null) => void
	setInfluence: (percent: number | null) => void
	setEvidenceWeight: (key: EvidenceWeightKey, weight: number | null) => void
	setHalfLife: (key: HalfLifeKey, days: number | null) => void
}

export interface ScoringActions extends AxisActions {
	add: () => void
	edit: (index: number) => void
	closeEdit: () => void
	setEnabled: (index: number, enabled: boolean) => void
	setName: (index: number, name: string) => void
	setDescription: (index: number, description: string) => void
	setId: (index: number, id: string) => void
	setProvider: (index: number, providerId: string) => void
	setWeight: (index: number, weight: number) => void
	setLearnedCalibration: (index: number, learned: boolean) => void
	setSamples: (index: number, samples: number | null) => void
	setSamplingTemperature: (index: number, temperature: number | null) => void
	setPrompt: (index: number, prompt: string) => void
	requestRemove: (index: number) => void
	confirmRemove: (index: number) => void
	cancelRemove: () => void
	togglePreview: () => void
	addCriterion: (index: number) => void
}
