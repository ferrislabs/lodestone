import { ExternalLink } from 'lucide-react'

import { externalHref } from '#/pages/leads/lib/format'

interface ProvenanceLinkProps {
	provenance: string
}

export function ProvenanceLink({ provenance }: ProvenanceLinkProps) {
	const href = externalHref(provenance)
	if (href === null) {
		return (
			<span className="break-all font-mono text-xs text-muted-foreground">
				{provenance}
			</span>
		)
	}
	return (
		<a
			href={href}
			target="_blank"
			rel="noopener noreferrer"
			className="inline-flex items-center gap-1 break-all text-xs text-orange-600 hover:underline"
		>
			{provenance}
			<ExternalLink className="size-3 shrink-0" />
		</a>
	)
}
