import { Button } from '#/components/ui/button'
import { formatDateTime } from '#/lib/format'
import {
	attestations,
	secretFields,
	secretState,
	settingFields,
} from '#/pages/source-providers/lib/form'
import { authLabel } from '#/pages/source-providers/lib/labels'
import type {
	FieldValues,
	ProviderDetail,
	SecretInputs,
} from '#/pages/source-providers/lib/types'
import { AccountPanel } from '#/pages/source-providers/ui/account-panel'
import { ConnectionPanel } from '#/pages/source-providers/ui/connection-panel'
import { FieldInput } from '#/pages/source-providers/ui/field-input'
import { SecretInput } from '#/pages/source-providers/ui/secret-input'
import { StatusBadge } from '#/pages/source-providers/ui/status-badge'

export interface Notice {
	kind: 'success' | 'error'
	text: string
}

interface ProviderPageProps {
	detail: ProviderDetail
	values: FieldValues
	secrets: SecretInputs
	replacing: readonly string[]
	errors: Readonly<Record<string, string>>
	formError: string | null
	notice: Notice | null
	dirty: boolean
	saving: boolean
	testing: boolean
	connecting: boolean
	disconnecting: boolean
	deleting: boolean
	confirmingDelete: boolean
	onChange: (key: string, value: string | boolean) => void
	onSecretChange: (key: string, value: string) => void
	onReplace: (key: string) => void
	onCancelReplace: (key: string) => void
	onSave: () => void
	onTest: () => void
	onConnect: () => void
	onDisconnect: () => void
	onRequestDelete: () => void
	onConfirmDelete: () => void
	onCancelDelete: () => void
	onBack: () => void
}

export function ProviderPage({
	detail,
	values,
	secrets,
	replacing,
	errors,
	formError,
	notice,
	dirty,
	saving,
	testing,
	connecting,
	disconnecting,
	deleting,
	confirmingDelete,
	onChange,
	onSecretChange,
	onReplace,
	onCancelReplace,
	onSave,
	onTest,
	onConnect,
	onDisconnect,
	onRequestDelete,
	onConfirmDelete,
	onCancelDelete,
	onBack,
}: ProviderPageProps) {
	const { summary } = detail
	const configured = summary.status !== 'not_configured'
	const authorizationCode = summary.auth_kind === 'oauth2_authorization_code'
	const hasFields = detail.fields.length > 0
	const declared = attestations(detail)
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div>
				<Button variant="ghost" size="sm" onClick={onBack}>
					Retour au catalogue
				</Button>
			</div>
			<div className="flex flex-col gap-1">
				<div className="flex items-center gap-3">
					<h1 className="text-2xl font-bold tracking-tight">{summary.name}</h1>
					<StatusBadge status={summary.status} />
				</div>
				<p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
					{authLabel(summary.auth_kind)}
				</p>
				<p className="text-sm text-muted-foreground">{summary.description}</p>
			</div>
			{notice ? (
				<p
					role={notice.kind === 'error' ? 'alert' : 'status'}
					className={
						notice.kind === 'error'
							? 'text-sm text-destructive'
							: 'text-sm text-emerald-700 dark:text-emerald-400'
					}
				>
					{notice.text}
				</p>
			) : null}
			<div className="grid max-w-4xl gap-4">
				{hasFields ? (
					<form
						aria-label="Configuration"
						autoComplete="off"
						className="flex flex-col gap-4 rounded-xl border bg-card p-4"
						onSubmit={(event) => {
							event.preventDefault()
							onSave()
						}}
					>
						<h2 className="text-base font-semibold">Configuration</h2>
						{settingFields(detail).map((field) => (
							<FieldInput
								key={field.key}
								field={field}
								value={values[field.key] ?? ''}
								error={errors[field.key]}
								onChange={onChange}
							/>
						))}
						{secretFields(detail).map((field) => (
							<SecretInput
								key={field.key}
								field={field}
								state={secretState(detail, field.key)}
								replacing={replacing.includes(field.key)}
								value={secrets[field.key] ?? ''}
								error={errors[field.key]}
								onChange={onSecretChange}
								onReplace={onReplace}
								onCancel={onCancelReplace}
							/>
						))}
						{formError ? (
							<p role="alert" className="text-sm text-destructive">
								{formError}
							</p>
						) : null}
						<div>
							<Button type="submit" disabled={saving || !dirty}>
								{saving ? 'Enregistrement…' : 'Enregistrer et tester'}
							</Button>
						</div>
					</form>
				) : (
					<p className="text-sm text-muted-foreground">
						Cette source n'a aucun réglage.
					</p>
				)}
				{authorizationCode ? (
					<AccountPanel
						detail={detail}
						configured={configured}
						connecting={connecting}
						disconnecting={disconnecting}
						onConnect={onConnect}
						onDisconnect={onDisconnect}
					/>
				) : null}
				<ConnectionPanel
					detail={detail}
					testing={testing}
					canTest={configured}
					onTest={onTest}
				/>
				{detail.terms_note ? (
					<section
						aria-label="Conditions d'utilisation"
						className="flex flex-col gap-1 rounded-xl border bg-muted/40 p-4"
					>
						<h2 className="text-base font-semibold">
							Conditions d'utilisation
						</h2>
						<p className="text-sm text-muted-foreground">{detail.terms_note}</p>
						{declared.map((attestation) => (
							<p
								key={attestation.key}
								className="text-sm text-muted-foreground"
							>
								{attestation.label} : déclaré le{' '}
								{formatDateTime(attestation.at)}
							</p>
						))}
					</section>
				) : null}
				{configured ? (
					<div className="flex items-center gap-2">
						{confirmingDelete ? (
							<>
								<Button
									type="button"
									variant="destructive"
									disabled={deleting}
									onClick={onConfirmDelete}
								>
									Confirmer la suppression
								</Button>
								<Button type="button" variant="ghost" onClick={onCancelDelete}>
									Annuler
								</Button>
							</>
						) : (
							<Button type="button" variant="outline" onClick={onRequestDelete}>
								Supprimer la configuration
							</Button>
						)}
					</div>
				) : null}
			</div>
		</div>
	)
}
