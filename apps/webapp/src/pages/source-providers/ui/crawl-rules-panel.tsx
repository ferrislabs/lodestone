import { TriangleAlert } from 'lucide-react'

import { Button } from '#/components/ui/button'
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '#/components/ui/dialog'
import {
	Sheet,
	SheetContent,
	SheetDescription,
	SheetHeader,
	SheetTitle,
} from '#/components/ui/sheet'
import { Skeleton } from '#/components/ui/skeleton'
import { Switch } from '#/components/ui/switch'
import {
	crawlChangeNote,
	crawlPolicyLabel,
	ignoresRobots,
	ROBOTS_WARNING,
	retriedNote,
	robotsIgnoredNote,
} from '#/pages/source-providers/lib/crawl'
import type { CrawlSettings } from '#/pages/source-providers/lib/types'

const SWITCH_ID = 'crawl-ignore-robots'

interface CrawlRulesSheetProps {
	open: boolean
	onOpenChange: (open: boolean) => void
	settings: CrawlSettings | null
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	saving: boolean
	saveError: string | null
	retried: number | null
	confirming: boolean
	onToggle: (ignore: boolean) => void
	onConfirm: () => void
	onCancel: () => void
}

export function CrawlRulesSheet({
	open,
	onOpenChange,
	settings,
	state,
	errorMessage,
	saving,
	saveError,
	retried,
	confirming,
	onToggle,
	onConfirm,
	onCancel,
}: CrawlRulesSheetProps) {
	const ignored = ignoresRobots(settings)
	const ignoredNote = settings
		? robotsIgnoredNote(settings.robots_ignored)
		: null
	const retriedText = retried === null ? null : retriedNote(retried)
	return (
		<>
			<Sheet open={open} onOpenChange={onOpenChange}>
				<SheetContent className="w-full gap-0 sm:max-w-lg">
					<SheetHeader className="border-b pr-12">
						<SheetTitle>Règles de collecte</SheetTitle>
						<SheetDescription>
							S'appliquent à toute l'organisation, pour la lecture des sites des
							entreprises.
						</SheetDescription>
					</SheetHeader>
					<div className="flex flex-col gap-4 p-4">
						{state === 'loading' ? <Skeleton className="h-24 w-full" /> : null}
						{state === 'error' ? (
							<p role="alert" className="text-sm text-destructive">
								{errorMessage ?? 'Impossible de charger les règles de collecte'}
							</p>
						) : null}
						{state === 'ready' && settings ? (
							<>
								<div className="flex items-start justify-between gap-4">
									<div className="flex flex-col gap-1">
										<label htmlFor={SWITCH_ID} className="text-sm font-medium">
											Ignorer robots.txt pour la lecture des sites
										</label>
										<p className="text-sm text-muted-foreground">
											{crawlPolicyLabel(ignored)}
										</p>
									</div>
									<Switch
										id={SWITCH_ID}
										checked={ignored}
										disabled={saving}
										onCheckedChange={onToggle}
									/>
								</div>
								<p className="text-xs text-muted-foreground">
									{crawlChangeNote(settings)}
								</p>
								{ignoredNote ? (
									<p className="text-xs text-muted-foreground">{ignoredNote}</p>
								) : null}
								{retriedText ? (
									<p className="text-xs text-muted-foreground">{retriedText}</p>
								) : null}
								{saveError ? (
									<p role="alert" className="text-sm text-destructive">
										{saveError}
									</p>
								) : null}
								<div className="flex gap-2 rounded-md border border-amber-500/50 bg-amber-500/10 p-3 text-sm">
									<TriangleAlert
										aria-hidden="true"
										className="mt-0.5 size-4 shrink-0 text-amber-600"
									/>
									<p role="note">{ROBOTS_WARNING}</p>
								</div>
							</>
						) : null}
					</div>
				</SheetContent>
			</Sheet>
			<Dialog
				open={confirming}
				onOpenChange={(next) => {
					if (!next) onCancel()
				}}
			>
				<DialogContent role="alertdialog">
					<DialogHeader>
						<DialogTitle>Ignorer robots.txt ?</DialogTitle>
						<DialogDescription>{ROBOTS_WARNING}</DialogDescription>
					</DialogHeader>
					<DialogFooter>
						<Button variant="outline" onClick={onCancel} disabled={saving}>
							Annuler
						</Button>
						<Button variant="destructive" onClick={onConfirm} disabled={saving}>
							Ignorer robots.txt
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	)
}
