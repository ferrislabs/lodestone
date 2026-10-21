import type { FormContext } from '#/components/schema-form/context'
import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import {
	Sheet,
	SheetContent,
	SheetDescription,
	SheetFooter,
	SheetHeader,
	SheetTitle,
} from '#/components/ui/sheet'
import type { JsonValue, Problem } from '#/lib/schema-form'
import {
	agentField,
	agentKind,
	agentTypeLabel,
	type DependencyNote,
	KIND_META,
} from '#/pages/profiles/lib/agents'
import { belongsToPath } from '#/pages/profiles/lib/tabs'
import { AgentForm } from '#/pages/profiles/ui/agent-form'
import type { SearchActions } from '#/pages/profiles/ui/search-agent-fields'

interface AgentSheetProps {
	context: FormContext
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	search: SearchActions
	onClose: () => void
	onEnabled: (enabled: boolean) => void
	onName: (name: string) => void
	onId: (id: string) => void
}

export function AgentSheet({
	context,
	agent,
	index,
	problems,
	note,
	search,
	onClose,
	onEnabled,
	onName,
	onId,
}: AgentSheetProps) {
	const kind = agentKind(agent)
	const name = agentField(agent, 'name')
	const own = problems.filter((problem) =>
		belongsToPath(problem.path, `agents[${index}]`),
	)
	return (
		<Sheet
			open
			onOpenChange={(open) => {
				if (!open) onClose()
			}}
		>
			<SheetContent className="w-full gap-0 sm:max-w-2xl">
				<SheetHeader className="border-b pr-12">
					<SheetTitle className="flex flex-wrap items-center gap-2 text-base">
						{name || 'Agent sans nom'}
						{kind ? (
							<Badge variant="secondary">{agentTypeLabel(agent)}</Badge>
						) : null}
						{kind && kind !== 'search' ? (
							<Badge variant="outline">{KIND_META[kind].label}</Badge>
						) : null}
						{own.length > 0 ? (
							<Badge variant="destructive">
								{own.length} {own.length > 1 ? 'problèmes' : 'problème'}
							</Badge>
						) : null}
					</SheetTitle>
					<SheetDescription>
						{kind ? KIND_META[kind].description : 'Réglages de cet agent.'}
					</SheetDescription>
				</SheetHeader>
				<div className="flex-1 overflow-y-auto p-4">
					<AgentForm
						context={context}
						agent={agent}
						index={index}
						problems={problems}
						note={note}
						search={search}
						onEnabled={onEnabled}
						onName={onName}
						onId={onId}
					/>
				</div>
				<SheetFooter className="flex-row justify-end border-t">
					<Button type="button" variant="outline" onClick={onClose}>
						Fermer
					</Button>
				</SheetFooter>
			</SheetContent>
		</Sheet>
	)
}
