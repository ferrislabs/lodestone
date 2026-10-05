import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
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
import type { Problem, ProfileSummary } from '#/pages/profiles/lib/types'
import { ProfileActivation } from '#/pages/profiles/ui/profile-activation'
import { ProfileProblems } from '#/pages/profiles/ui/profile-problems'

const SKELETON_ROWS = ['a', 'b', 'c']
const COLUMNS = 5

interface ProfilesListProps {
	profiles: readonly ProfileSummary[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	actionError: string | null
	busyId: string | null
	confirmingId: string | null
	importName: string
	importing: boolean
	importProblems: readonly Problem[]
	importError: string | null
	onImportNameChange: (name: string) => void
	onImportFile: (file: File) => void
	onCreate: () => void
	onOpen: (profileId: string) => void
	onActivate: (profileId: string) => void
	onRequestDeactivate: (profileId: string) => void
	onConfirmDeactivate: (profileId: string) => void
	onCancelDeactivate: () => void
}

export function ProfilesList({
	profiles,
	state,
	errorMessage,
	actionError,
	busyId,
	confirmingId,
	importName,
	importing,
	importProblems,
	importError,
	onImportNameChange,
	onImportFile,
	onCreate,
	onOpen,
	onActivate,
	onRequestDeactivate,
	onConfirmDeactivate,
	onCancelDeactivate,
}: ProfilesListProps) {
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="flex flex-col gap-1">
					<h1 className="text-2xl font-bold tracking-tight">Profils</h1>
					<p className="text-sm text-muted-foreground">
						Les profils de détection décrivent un marché. Chaque modification
						crée une nouvelle révision.
					</p>
				</div>
				<Button onClick={onCreate}>Nouveau profil</Button>
			</div>
			<div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
				<h2 className="text-sm font-semibold">Importer un profil</h2>
				<div className="flex flex-wrap items-end gap-3">
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="import-file">Fichier JSON</Label>
						<Input
							id="import-file"
							type="file"
							accept="application/json,.json"
							disabled={importing}
							onChange={(event) => {
								const file = event.target.files?.[0]
								if (file) onImportFile(file)
								event.target.value = ''
							}}
						/>
					</div>
					<div className="flex flex-col gap-1.5">
						<Label htmlFor="import-name">Nom (facultatif)</Label>
						<Input
							id="import-name"
							value={importName}
							placeholder="Par défaut, l'offre"
							onChange={(event) => onImportNameChange(event.target.value)}
						/>
					</div>
				</div>
				{importError ? (
					<p role="alert" className="text-sm text-destructive">
						{importError}
					</p>
				) : null}
				<ProfileProblems
					problems={importProblems}
					title="Le fichier a été rejeté"
				/>
			</div>
			{actionError ? (
				<p role="alert" className="text-sm text-destructive">
					{actionError}
				</p>
			) : null}
			<div className="rounded-xl border bg-card">
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Nom</TableHead>
							<TableHead>Statut</TableHead>
							<TableHead>Révision</TableHead>
							<TableHead>Leads</TableHead>
							<TableHead className="text-right">Actions</TableHead>
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
									Impossible de charger les profils : {errorMessage}
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready' && profiles.length === 0 ? (
							<TableRow>
								<TableCell
									colSpan={COLUMNS}
									className="py-10 text-center text-muted-foreground"
								>
									Aucun profil pour le moment.
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready'
							? profiles.map((profile) => (
									<TableRow key={profile.id}>
										<TableCell>
											<button
												type="button"
												className="font-medium text-foreground hover:underline"
												onClick={() => onOpen(profile.id)}
											>
												{profile.name}
											</button>
										</TableCell>
										<TableCell>
											<Badge variant={profile.active ? 'default' : 'secondary'}>
												{profile.active ? 'Actif' : 'Inactif'}
											</Badge>
										</TableCell>
										<TableCell className="text-muted-foreground">
											{profile.revision ?? '-'}
										</TableCell>
										<TableCell className="text-muted-foreground">
											{profile.lead_count}
										</TableCell>
										<TableCell className="flex justify-end">
											<ProfileActivation
												active={profile.active}
												confirming={confirmingId === profile.id}
												busy={busyId === profile.id}
												onActivate={() => onActivate(profile.id)}
												onRequestDeactivate={() =>
													onRequestDeactivate(profile.id)
												}
												onConfirmDeactivate={() =>
													onConfirmDeactivate(profile.id)
												}
												onCancelDeactivate={onCancelDeactivate}
											/>
										</TableCell>
									</TableRow>
								))
							: null}
					</TableBody>
				</Table>
			</div>
		</div>
	)
}
