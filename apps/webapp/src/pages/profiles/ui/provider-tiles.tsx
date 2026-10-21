import { CheckIcon } from 'lucide-react'
import type { MouseEvent } from 'react'

import { cn } from '#/lib/utils'
import type { ProviderTile } from '#/pages/profiles/lib/provider-tiles'
import { statusLabel } from '#/pages/source-providers/lib/labels'
import { providerIcon } from '#/pages/source-providers/lib/provider-icons'
import { statusBorder } from '#/pages/source-providers/ui/provider-card'

interface ProviderLogoProps {
	providerKey: string
	name: string
}

export function ProviderLogo({ providerKey, name }: ProviderLogoProps) {
	const icon = providerIcon(providerKey)
	return (
		<span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-white p-1.5 ring-1 ring-border">
			{icon.kind === 'logo' ? (
				<img src={icon.src} alt={name} className="size-full object-contain" />
			) : (
				<icon.Icon aria-hidden="true" className="size-6 text-neutral-700" />
			)}
		</span>
	)
}

interface ConfigureLinkProps {
	providerKey: string
	name: string
	onOpen: (key: string) => void
}

export function ConfigureLink({
	providerKey,
	name,
	onOpen,
}: ConfigureLinkProps) {
	return (
		<a
			href={`/sources/providers/${providerKey}`}
			aria-label={`Configurer ${name}`}
			className="text-xs font-medium text-primary underline-offset-4 hover:underline"
			onClick={(event: MouseEvent<HTMLAnchorElement>) => {
				if (
					event.button !== 0 ||
					event.metaKey ||
					event.ctrlKey ||
					event.shiftKey ||
					event.altKey
				)
					return
				event.preventDefault()
				onOpen(providerKey)
			}}
		>
			À configurer
		</a>
	)
}

interface ProviderTilesProps {
	tiles: readonly ProviderTile[]
	label: string
	onToggle: (key: string, attached: boolean) => void
	onOpenProvider: (key: string) => void
}

const TILE =
	'relative flex aspect-square w-full flex-col items-center justify-center gap-1 rounded-xl border bg-card p-2 text-center text-xs'

const SELECTED =
	'border-transparent bg-emerald-50 ring-2 ring-emerald-500 ring-offset-0 hover:border-transparent dark:bg-emerald-950/40'

export function ProviderTiles({
	tiles,
	label,
	onToggle,
	onOpenProvider,
}: ProviderTilesProps) {
	return (
		<ul
			aria-label={label}
			className="grid grid-cols-[repeat(auto-fill,minmax(120px,136px))] gap-2.5"
		>
			{tiles.map((tile) => {
				const selected = tile.state === 'selected'
				return (
					<li
						key={tile.key}
						data-provider-tile={tile.key}
						data-state={tile.state}
						data-status={tile.status}
						title={`${tile.name} — ${statusLabel(tile.status)}`}
						className={cn(
							TILE,
							selected ? SELECTED : statusBorder(tile.status),
							tile.state === 'unready' && 'border-dashed text-muted-foreground',
						)}
					>
						{tile.selectable ? (
							<button
								type="button"
								aria-pressed={selected}
								aria-label={tile.name}
								className="flex size-full flex-col items-center justify-center gap-1 rounded-lg outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-dashed focus-visible:outline-muted-foreground/60"
								onClick={() => onToggle(tile.key, !selected)}
							>
								<ProviderLogo providerKey={tile.key} name={tile.name} />
								<span className="font-medium leading-tight">{tile.name}</span>
								{selected ? (
									<span className="inline-flex items-center gap-0.5 font-medium text-emerald-700 dark:text-emerald-400">
										<CheckIcon className="size-3" />
										Ajouté
									</span>
								) : null}
							</button>
						) : (
							<>
								<ProviderLogo providerKey={tile.key} name={tile.name} />
								<span className="font-medium leading-tight">{tile.name}</span>
							</>
						)}
						{tile.ready ? null : (
							<ConfigureLink
								providerKey={tile.key}
								name={tile.name}
								onOpen={onOpenProvider}
							/>
						)}
					</li>
				)
			})}
		</ul>
	)
}
