import { HelpPanel } from '#/pages/profiles/ui/help-panel'

interface AgentsHelpProps {
	collapsed: boolean
	onToggle: () => void
}

const FAMILIES: readonly { title: string; sources: string }[] = [
	{
		title: 'Infrastructure',
		sources: 'Transparence des certificats, DNS CNAME, Sonde HTTP',
	},
	{ title: 'Marchés publics', sources: 'BOAMP, DECP, TED' },
	{ title: "Offres d'emploi", sources: 'France Travail, ATS' },
]

export function AgentsHelp({ collapsed, onToggle }: AgentsHelpProps) {
	return (
		<HelpPanel title="Agents" collapsed={collapsed} onToggle={onToggle}>
			<p>
				Un agent interroge une source avec ses propres réglages. Il est possible
				d'avoir plusieurs agents du même type.
			</p>
			<dl className="flex flex-col gap-1.5">
				{FAMILIES.map((family) => (
					<div key={family.title}>
						<dt className="font-medium">{family.title}</dt>
						<dd className="text-xs text-muted-foreground">{family.sources}</dd>
					</div>
				))}
			</dl>
			<div className="flex flex-col gap-1">
				<p className="font-medium">Dépendances</p>
				<ul className="flex list-disc flex-col gap-1 pl-4 text-xs">
					<li>Sonde HTTP ← Transparence des certificats</li>
					<li>ATS ← entreprises déjà détectées</li>
				</ul>
			</div>
			<p className="text-xs text-muted-foreground">
				Les adresses de contact à privilégier et à deviner se règlent en bas de
				cet onglet. Les critères de pertinence se règlent dans le prompt des
				agents de qualification.
			</p>
		</HelpPanel>
	)
}
