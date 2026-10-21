import { AGENT_KINDS, KIND_META } from '#/pages/profiles/lib/agents'
import { HelpPanel } from '#/pages/profiles/ui/help-panel'

interface AgentsHelpProps {
	collapsed: boolean
	onToggle: () => void
}

export function AgentsHelp({ collapsed, onToggle }: AgentsHelpProps) {
	return (
		<HelpPanel title="Agents" collapsed={collapsed} onToggle={onToggle}>
			<p>
				Le catalogue liste les sources disponibles pour toute l'organisation ;
				un agent décide quoi chercher, et dans quelles sources.
			</p>
			<p className="text-xs text-muted-foreground">
				Les agents de sourcing trouvent les leads. Trois types existent :
			</p>
			<dl className="flex flex-col gap-1.5">
				{AGENT_KINDS.map((kind) => (
					<div key={kind}>
						<dt className="font-medium">{KIND_META[kind].label}</dt>
						<dd className="text-xs text-muted-foreground">
							{KIND_META[kind].description}
						</dd>
					</div>
				))}
			</dl>
			<p className="text-xs text-muted-foreground">
				La sonde HTTP, les offres d'emploi de l'entreprise et le dossier
				entreprise complètent chaque lead trouvé : ils se règlent dans l'onglet
				Enrichissement.
			</p>
			<p className="text-xs text-muted-foreground">
				Les adresses de contact à privilégier et à deviner se règlent en bas de
				cet onglet. Les critères de pertinence se règlent dans le prompt des
				agents de qualification.
			</p>
		</HelpPanel>
	)
}
