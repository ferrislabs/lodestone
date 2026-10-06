import { tierShortLabel } from '#/pages/llm/lib/labels'
import {
	TIERS,
	type TierAction,
	type TierAssignment,
	type TierName,
	type TierSelection,
} from '#/pages/llm/lib/types'

export const NO_PROVIDER = ''

export function selectionFromAssignments(
	assignments: readonly TierAssignment[],
): TierSelection {
	const selection: TierSelection = {
		triage: NO_PROVIDER,
		qualification: NO_PROVIDER,
	}
	for (const assignment of assignments) {
		if (isTier(assignment.tier)) {
			selection[assignment.tier] = assignment.provider_id ?? NO_PROVIDER
		}
	}
	return selection
}

function isTier(value: string): value is TierName {
	return TIERS.some((tier) => tier === value)
}

export function tierDiff(
	current: TierSelection,
	desired: TierSelection,
): TierAction[] {
	const actions: TierAction[] = []
	for (const tier of TIERS) {
		if (current[tier] === desired[tier]) continue
		actions.push(
			desired[tier] === NO_PROVIDER
				? { tier, op: 'unassign' }
				: { tier, op: 'assign', providerId: desired[tier] },
		)
	}
	return actions
}

export function deleteConflictMessage(tiers: readonly string[]): string {
	if (tiers.length === 0) {
		return 'Suppression impossible : le fournisseur est utilisé'
	}
	const names = tiers.map(tierShortLabel).join(' et ')
	return `Suppression impossible : fournisseur par défaut pour ${names}`
}

export function keyLabel(hasApiKey: boolean): string {
	return hasApiKey ? 'Clé définie' : 'Aucune clé'
}
