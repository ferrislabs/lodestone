import { z } from 'zod'

import type { Problem } from '#/pages/profiles/lib/types'

export const EDITOR_TABS = [
	'identity',
	'agents',
	'scoring',
	'settings',
] as const

export type EditorTab = (typeof EDITOR_TABS)[number]
export type ProfileTab = EditorTab | 'history'

export const TAB_LABELS: Record<ProfileTab, string> = {
	identity: 'Identité',
	agents: 'Agents',
	scoring: 'Scoring',
	settings: 'Réglages',
	history: 'Historique des révisions',
}

export const tabSearchSchema = z.object({
	tab: z
		.enum([...EDITOR_TABS, 'history'])
		.optional()
		.catch(undefined),
})

export interface TabItem<T extends string = ProfileTab> {
	id: T
	label: string
	problems: number
}

function belongsTo(path: string, root: string): boolean {
	return (
		path === root || path.startsWith(`${root}.`) || path.startsWith(`${root}[`)
	)
}

export function tabOfPath(path: string): EditorTab | null {
	if (belongsTo(path, 'identity')) return 'identity'
	if (belongsTo(path, 'agents')) return 'agents'
	if (belongsTo(path, 'scoring')) return 'scoring'
	if (belongsTo(path, 'llm') || belongsTo(path, 'contact_ranking')) {
		return 'settings'
	}
	return null
}

export function problemCounts(
	problems: readonly Problem[],
): Record<EditorTab, number> {
	const counts: Record<EditorTab, number> = {
		identity: 0,
		agents: 0,
		scoring: 0,
		settings: 0,
	}
	for (const problem of problems) {
		const tab = tabOfPath(problem.path)
		if (tab !== null) counts[tab] += 1
	}
	return counts
}

export function editorTabItems(
	problems: readonly Problem[],
): TabItem<EditorTab>[] {
	const counts = problemCounts(problems)
	return EDITOR_TABS.map((id) => ({
		id,
		label: TAB_LABELS[id],
		problems: counts[id],
	}))
}

export function asEditorTab(tab: ProfileTab | undefined): EditorTab {
	return tab === undefined || tab === 'history' ? 'identity' : tab
}

export function belongsToPath(path: string, root: string): boolean {
	return belongsTo(path, root)
}
