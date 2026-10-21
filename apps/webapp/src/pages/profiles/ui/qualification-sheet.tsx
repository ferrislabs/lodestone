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
import { agentField } from '#/pages/profiles/lib/agents'
import type { ProviderOption } from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { belongsToPath } from '#/pages/profiles/lib/tabs'
import { QualificationForm } from '#/pages/profiles/ui/qualification-form'

interface QualificationSheetProps {
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	previewing: boolean
	actions: QualificationActions
}

export function QualificationSheet({
	agent,
	index,
	problems,
	providers,
	previewing,
	actions,
}: QualificationSheetProps) {
	const name = agentField(agent, 'name')
	const own = problems.filter((problem) =>
		belongsToPath(problem.path, `qualification_agents[${index}]`),
	)
	return (
		<Sheet
			open
			onOpenChange={(open) => {
				if (!open) actions.closeEdit()
			}}
		>
			<SheetContent className="w-full gap-0 sm:max-w-2xl">
				<SheetHeader className="border-b pr-12">
					<SheetTitle className="flex flex-wrap items-center gap-2 text-base">
						{name || 'Agent sans nom'}
						<Badge variant="secondary">Qualification</Badge>
						{own.length > 0 ? (
							<Badge variant="destructive">
								{own.length} {own.length > 1 ? 'problèmes' : 'problème'}
							</Badge>
						) : null}
					</SheetTitle>
					<SheetDescription>
						Évalue chaque lead du profil sous un angle, en répondant à des
						questions à choix.
					</SheetDescription>
				</SheetHeader>
				<div className="flex-1 overflow-y-auto p-4">
					<QualificationForm
						agent={agent}
						index={index}
						problems={problems}
						providers={providers}
						previewing={previewing}
						actions={actions}
					/>
				</div>
				<SheetFooter className="flex-row justify-end border-t">
					<Button type="button" variant="outline" onClick={actions.closeEdit}>
						Fermer
					</Button>
				</SheetFooter>
			</SheetContent>
		</Sheet>
	)
}
