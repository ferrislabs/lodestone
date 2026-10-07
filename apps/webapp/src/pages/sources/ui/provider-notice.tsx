import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import {
	PROVIDER_NOT_READY_LABEL,
	providerReady,
} from '#/pages/sources/lib/state'
import type { Source } from '#/pages/sources/lib/types'

interface ProviderNoticeProps {
	source: Source
	onOpenProvider: (key: string) => void
}

export function ProviderNotice({
	source,
	onOpenProvider,
}: ProviderNoticeProps) {
	const provider = source.provider
	if (provider == null || providerReady(source)) return null
	return (
		<div className="flex flex-col items-start gap-1">
			<Badge variant="destructive">{PROVIDER_NOT_READY_LABEL}</Badge>
			<Button
				type="button"
				variant="link"
				size="sm"
				className="h-auto p-0 text-xs"
				onClick={() => onOpenProvider(provider.key)}
			>
				Ouvrir {provider.name} dans le catalogue
			</Button>
		</div>
	)
}
