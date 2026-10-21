import { Button } from '#/components/ui/button'
import { formatDateTime } from '#/lib/format'
import type { ProviderDetail } from '#/pages/source-providers/lib/types'

interface AccountPanelProps {
	detail: ProviderDetail
	configured: boolean
	connecting: boolean
	disconnecting: boolean
	onConnect: () => void
	onDisconnect: () => void
}

export function AccountPanel({
	detail,
	configured,
	connecting,
	disconnecting,
	onConnect,
	onDisconnect,
}: AccountPanelProps) {
	const account = detail.summary.connected_account
	return (
		<section
			aria-label="Compte connecté"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4"
		>
			<h2 className="text-base font-semibold">Compte connecté</h2>
			{detail.oauth_redirect_uri ? (
				<p className="text-xs text-muted-foreground">
					URL de rappel à déclarer chez le fournisseur :{' '}
					<code className="break-all">{detail.oauth_redirect_uri}</code>
				</p>
			) : null}
			{account ? (
				<>
					<dl className="grid gap-1 text-sm">
						<div className="flex gap-2">
							<dt className="text-muted-foreground">Compte</dt>
							<dd className="font-medium">{account.display_name}</dd>
						</div>
						{account.account_id ? (
							<div className="flex gap-2">
								<dt className="text-muted-foreground">Identifiant</dt>
								<dd>{account.account_id}</dd>
							</div>
						) : null}
						{account.scopes.length > 0 ? (
							<div className="flex gap-2">
								<dt className="text-muted-foreground">Autorisations</dt>
								<dd>{account.scopes.join(', ')}</dd>
							</div>
						) : null}
						{account.expires_at ? (
							<div className="flex gap-2">
								<dt className="text-muted-foreground">
									Jeton valable jusqu'au
								</dt>
								<dd>{formatDateTime(account.expires_at)}</dd>
							</div>
						) : null}
					</dl>
					<div>
						<Button
							type="button"
							variant="outline"
							disabled={disconnecting}
							onClick={onDisconnect}
						>
							{disconnecting ? 'Déconnexion…' : 'Déconnecter'}
						</Button>
					</div>
				</>
			) : (
				<>
					<p className="text-sm text-muted-foreground">
						Aucun compte n'est connecté.
						{configured
							? ''
							: " Enregistrez d'abord l'identifiant et le secret du client."}
					</p>
					<div>
						<Button
							type="button"
							disabled={!configured || connecting}
							onClick={onConnect}
						>
							{connecting ? 'Redirection…' : 'Connecter un compte'}
						</Button>
					</div>
				</>
			)}
		</section>
	)
}
