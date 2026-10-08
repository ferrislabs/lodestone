import { z } from 'zod'

import type { Problem } from '#/pages/profiles/lib/types'

export const EDITOR_TABS = [
	'identity',
	'sourcing',
	'enrichment',
	'qualification',
	'scoring',
] as const

export type EditorTab = (typeof EDITOR_TABS)[number]
export type ProfileTab = EditorTab | 'history'

export const TAB_LABELS: Record<ProfileTab, string> = {
	identity: 'Identité',
	sourcing: 'Sourcing',
	enrichment: 'Enrichissement',
	qualification: 'Qualification',
	scoring: 'Scoring',
	history: 'Historique des révisions',
}

const LEGACY_TABS: Readonly<Record<string, ProfileTab>> = {
	agents: 'sourcing',
	settings: 'identity',
}

export const tabSearchSchema = z.object({
	tab: z
		.preprocess(
			(raw) => (typeof raw === 'string' ? (LEGACY_TABS[raw] ?? raw) : raw),
			z.enum([...EDITOR_TABS, 'history']),
		)
		.optional()
		.catch(undefined),
	agent: z.string().optional().catch(undefined),
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
	if (belongsTo(path, 'identity') || belongsTo(path, 'language')) {
		return 'identity'
	}
	if (belongsTo(path, 'agents') || belongsTo(path, 'contact_ranking')) {
		return 'sourcing'
	}
	if (belongsTo(path, 'enrichment') || belongsTo(path, 'inspection')) {
		return 'enrichment'
	}
	if (
		belongsTo(path, 'qualification_agents') ||
		belongsTo(path, 'aggregator') ||
		belongsTo(path, 'verdict') ||
		belongsTo(path, 'evidence')
	) {
		return 'qualification'
	}
	if (belongsTo(path, 'scoring_agents')) {
		return 'scoring'
	}
	return null
}

export function problemCounts(
	problems: readonly Problem[],
): Record<EditorTab, number> {
	const counts: Record<EditorTab, number> = {
		identity: 0,
		sourcing: 0,
		enrichment: 0,
		qualification: 0,
		scoring: 0,
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
