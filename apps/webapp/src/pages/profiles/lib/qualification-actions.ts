export interface QualificationActions {
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
	setWeight: (index: number, weight: number) => void
	setPrompt: (index: number, prompt: string) => void
	requestRemove: (index: number) => void
	confirmRemove: (index: number) => void
	cancelRemove: () => void
	togglePreview: () => void
	addQuestion: (index: number) => void
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
	setAggregatorProvider: (providerId: string) => void
	setAggregatorPrompt: (prompt: string) => void
	setReliableFrom: (percent: number | null) => void
	setDiscardBelow: (percent: number | null) => void
}
