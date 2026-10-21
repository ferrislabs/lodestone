import { isJsonObject, type JsonValue } from '#/lib/schema-form'
import {
	agentField,
	agentKind,
	agentProviders,
} from '#/pages/profiles/lib/agents'
import { agentKeywordCount } from '#/pages/profiles/lib/search-agent'

export type WizardStep = 'type' | 'criteria' | 'summary'

export const WIZARD_STEPS: readonly { id: WizardStep; label: string }[] = [
	{ id: 'type', label: 'Type et fournisseurs' },
	{ id: 'criteria', label: 'Critères' },
	{ id: 'summary', label: 'Récapitulatif' },
]

const ORDER: readonly WizardStep[] = WIZARD_STEPS.map((step) => step.id)

export function nextStep(step: WizardStep): WizardStep {
	return ORDER[Math.min(ORDER.indexOf(step) + 1, ORDER.length - 1)] ?? step
}

export function previousStep(step: WizardStep): WizardStep {
	return ORDER[Math.max(ORDER.indexOf(step) - 1, 0)] ?? step
}

function typeProblems(agent: JsonValue): string[] {
	return agentKind(agent) === 'search' && agentProviders(agent).length === 0
		? ['Choisir au moins un fournisseur.']
		: []
}

function filled(item: JsonValue, keys: readonly string[]): boolean {
	return (
		isJsonObject(item) &&
		keys.every((key) => {
			const field = item[key]
			return typeof field === 'string' && field.trim() !== ''
		})
	)
}

function list(agent: JsonValue, key: string): JsonValue[] {
	if (!isJsonObject(agent)) return []
	const items = agent[key]
	return Array.isArray(items) ? items : []
}

function criteriaProblems(agent: JsonValue): string[] {
	switch (agentKind(agent)) {
		case 'search':
			return agentKeywordCount(agent) === 0
				? ['Ajouter au moins un mot-clé.']
				: []
		case 'certificates':
			return list(agent, 'labels').some(
				(label) => typeof label === 'string' && label.trim() !== '',
			)
				? []
				: ['Ajouter au moins un libellé.']
		case 'dns':
			return list(agent, 'patterns').some((pattern) =>
				filled(pattern, ['provider', 'target_suffix']),
			)
				? []
				: ['Ajouter au moins un motif CNAME.']
		default:
			return []
	}
}

export function stepProblems(step: WizardStep, agent: JsonValue): string[] {
	if (step === 'type') return typeProblems(agent)
	if (step === 'criteria') return criteriaProblems(agent)
	const name =
		agentField(agent, 'name').trim() === '' ? ["Donner un nom à l'agent."] : []
	return [...typeProblems(agent), ...criteriaProblems(agent), ...name]
}
