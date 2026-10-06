import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Label } from '#/components/ui/label'
import { Skeleton } from '#/components/ui/skeleton'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { pluralize } from '#/lib/labels'
import { previewText } from '#/pages/drafts/lib/edit'
import type {
	DraftEntry,
	PaginationMeta,
	ProfileSummary,
} from '#/pages/drafts/lib/types'
import { formatDateTime } from '#/pages/leads/lib/format'
import {
	VERDICT_KEYS,
	type VerdictKey,
	verdictKeyLabel,
} from '#/pages/leads/lib/verdict'
import { ConfidenceBar } from '#/pages/leads/ui/confidence-bar'
import { ScoreBar } from '#/pages/leads/ui/score-bar'
import { VerdictBadge } from '#/pages/leads/ui/verdict-badge'

const SKELETON_ROWS = ['a', 'b', 'c', 'd', 'e']
const COLUMNS = 8

const SELECT_CLASS =
	'h-9 w-full rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'

interface DraftsListProps {
	entries: readonly DraftEntry[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	profiles: readonly ProfileSummary[]
	profileId: string
	onProfileChange: (profileId: string) => void
	verdicts: readonly VerdictKey[]
	onVerdictsChange: (verdicts: VerdictKey[]) => void
	pagination: PaginationMeta | null
	onPageChange: (page: number) => void
	onOpen: (assessmentId: string) => void
}

export function DraftsList({
	entries,
	state,
	errorMessage,
	profiles,
	profileId,
	onProfileChange,
	verdicts,
	onVerdictsChange,
	pagination,
	onPageChange,
	onOpen,
}: DraftsListProps) {
	const maxScore = entries.reduce(
		(max, entry) => Math.max(max, entry.lead.score),
		0,
	)
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div className="flex flex-col gap-1">
				<h1 className="text-2xl font-bold tracking-tight">Brouillons</h1>
				<p className="text-sm text-muted-foreground">
					Brouillons de messages en attente de relecture. Lodestone n'envoie
					jamais de message : approuver marque seulement un brouillon comme
					prêt.
				</p>
			</div>
			<div className="flex flex-wrap items-end gap-x-8 gap-y-3">
				<div className="flex w-full max-w-sm flex-col gap-1.5">
					<Label htmlFor="draft-profile">Profil</Label>
					<select
						id="draft-profile"
						className={SELECT_CLASS}
						value={profileId}
						onChange={(event) => onProfileChange(event.target.value)}
					>
						<option value="">Tous les profils</option>
						{profiles.map((profile) => (
							<option key={profile.id} value={profile.id}>
								{profile.name}
							</option>
						))}
					</select>
				</div>
				<fieldset className="flex flex-col gap-1.5">
					<legend className="text-sm font-medium">Verdict</legend>
					<div className="flex flex-wrap gap-x-4 gap-y-1">
						{VERDICT_KEYS.map((key) => (
							<label key={key} className="flex items-center gap-1.5 text-sm">
								<input
									type="checkbox"
									name="verdict"
									value={key}
									checked={verdicts.includes(key)}
									onChange={(event) =>
										onVerdictsChange(
											event.target.checked
												? [...verdicts, key]
												: verdicts.filter((current) => current !== key),
										)
									}
								/>
								{verdictKeyLabel(key)}
							</label>
						))}
					</div>
				</fieldset>
			</div>
			<div className="rounded-xl border bg-card">
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Lead</TableHead>
							<TableHead>Profil</TableHead>
							<TableHead>Verdict</TableHead>
							<TableHead>Confiance</TableHead>
							<TableHead>Score</TableHead>
							<TableHead>Objet</TableHead>
							<TableHead>Contact</TableHead>
							<TableHead>Créé le</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{state === 'loading'
							? SKELETON_ROWS.map((id) => (
									<TableRow key={id}>
										<TableCell colSpan={COLUMNS}>
											<Skeleton className="h-5 w-full" />
										</TableCell>
									</TableRow>
								))
							: null}
						{state === 'error' ? (
							<TableRow>
								<TableCell
									colSpan={COLUMNS}
									className="py-10 text-center text-destructive"
									role="alert"
								>
									Impossible de charger les brouillons : {errorMessage}
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready' && entries.length === 0 ? (
							<TableRow>
								<TableCell
									colSpan={COLUMNS}
									className="py-10 text-center text-muted-foreground"
								>
									Aucun brouillon en attente de relecture.
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready'
							? entries.map((entry) => (
									<TableRow key={entry.assessment_id}>
										<TableCell>
											<button
												type="button"
												className="font-medium text-foreground hover:underline"
												onClick={() => onOpen(entry.assessment_id)}
											>
												{entry.lead.domain}
											</button>
										</TableCell>
										<TableCell>
											<Badge variant="secondary">
												{entry.lead.profile.name}
											</Badge>
										</TableCell>
										<TableCell>
											<VerdictBadge verdict={entry.lead.verdict} />
										</TableCell>
										<TableCell>
											<ConfidenceBar confidence={entry.lead.confidence} />
										</TableCell>
										<TableCell>
											<ScoreBar score={entry.lead.score} max={maxScore} />
										</TableCell>
										<TableCell className="max-w-xs truncate">
											{previewText(entry.draft.subject, 80)}
										</TableCell>
										<TableCell className="text-muted-foreground">
											{entry.contact ? entry.contact.address : 'Aucun contact'}
										</TableCell>
										<TableCell className="text-muted-foreground">
											{formatDateTime(entry.created_at)}
										</TableCell>
									</TableRow>
								))
							: null}
					</TableBody>
				</Table>
			</div>
			{pagination && state === 'ready' ? (
				<div className="flex items-center justify-between text-sm text-muted-foreground">
					<span>
						Page {pagination.current_page}
						{pagination.last_page ? ` sur ${pagination.last_page}` : ''}
						{pagination.total === null || pagination.total === undefined
							? ''
							: ` - ${pagination.total} ${pluralize(pagination.total, 'brouillon', 'brouillons')}`}
					</span>
					<div className="flex gap-2">
						<Button
							variant="outline"
							size="sm"
							disabled={!pagination.prev_page}
							onClick={() => onPageChange(pagination.prev_page ?? 1)}
						>
							Précédent
						</Button>
						<Button
							variant="outline"
							size="sm"
							disabled={!pagination.next_page}
							onClick={() =>
								onPageChange(pagination.next_page ?? pagination.current_page)
							}
						>
							Suivant
						</Button>
					</div>
				</div>
			) : null}
		</div>
	)
}
