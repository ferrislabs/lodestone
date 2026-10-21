import { Badge } from '#/components/ui/badge'
import type { TechGroup } from '#/lib/ecosystem-views'

interface TechGroupsProps {
	groups: readonly TechGroup[]
}

function chipTitle(evidence: string | null, sourceUrl: string): string {
	return evidence === null ? sourceUrl : `${evidence} · ${sourceUrl}`
}

export function TechGroups({ groups }: TechGroupsProps) {
	return (
		<dl className="flex flex-col gap-2" data-tech-groups>
			{groups.map((group) => (
				<div
					key={group.category}
					data-category={group.category}
					className="flex flex-col gap-1"
				>
					<dt className="text-xs text-muted-foreground">{group.label}</dt>
					<dd className="flex flex-wrap gap-1">
						{group.chips.map((chip) => (
							<Badge
								key={chip.key}
								variant="outline"
								data-tech={chip.key}
								title={chipTitle(chip.evidence, chip.sourceUrl)}
							>
								{chip.version ? `${chip.name} ${chip.version}` : chip.name}
							</Badge>
						))}
					</dd>
				</div>
			))}
		</dl>
	)
}
