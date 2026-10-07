import { Button } from '#/components/ui/button'
import {
	type AgentKind,
	KIND_GROUPS,
	KIND_META,
} from '#/pages/profiles/lib/agents'

interface AgentPickerProps {
	onPick: (kind: AgentKind) => void
	onCancel: () => void
}

export function AgentPicker({ onPick, onCancel }: AgentPickerProps) {
	return (
		<section
			aria-label="Choisir un type d'agent"
			className="flex flex-col gap-4 rounded-md border bg-muted/20 p-3"
		>
			<div className="flex items-center justify-between">
				<p className="text-sm font-semibold">Quel type d'agent ajouter ?</p>
				<Button type="button" variant="ghost" size="sm" onClick={onCancel}>
					Annuler
				</Button>
			</div>
			{KIND_GROUPS.map((group) => (
				<div key={group.title} className="flex flex-col gap-2">
					<p className="text-xs font-medium uppercase text-muted-foreground">
						{group.title}
					</p>
					<ul className="grid gap-2 md:grid-cols-3">
						{group.kinds.map((kind) => (
							<li key={kind}>
								<button
									type="button"
									className="flex h-full w-full flex-col gap-0.5 rounded-md border bg-background p-3 text-left hover:border-primary"
									onClick={() => onPick(kind)}
								>
									<span className="text-sm font-medium">
										{KIND_META[kind].label}
									</span>
									<span className="text-xs text-muted-foreground">
										{KIND_META[kind].description}
									</span>
								</button>
							</li>
						))}
					</ul>
				</div>
			))}
		</section>
	)
}
