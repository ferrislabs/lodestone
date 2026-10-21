import { Button } from '#/components/ui/button'
import { formatDateTime } from '#/lib/format'
import type { ProviderDetail } from '#/pages/source-providers/lib/types'
import { StatusBadge } from '#/pages/source-providers/ui/status-badge'

interface ConnectionPanelProps {
	detail: ProviderDetail
	testing: boolean
	canTest: boolean
	onTest: () => void
}

export function ConnectionPanel({
	detail,
	testing,
	canTest,
	onTest,
}: ConnectionPanelProps) {
	const { summary } = detail
	return (
		<section
			aria-label="Connexion"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4"
		>
			<div className="flex items-center justify-between gap-2">
				<h2 className="text-base font-semibold">Connexion</h2>
				<StatusBadge status={summary.status} />
			</div>
			{summary.last_test_at ? (
				<p className="text-sm text-muted-foreground">
					Dernier test : {formatDateTime(summary.last_test_at)}
				</p>
			) : (
				<p className="text-sm text-muted-foreground">
					La connexion n'a pas encore été testée.
				</p>
			)}
			{detail.last_test_message ? (
				<output
					className={
						summary.status === 'failing'
							? 'block text-sm text-destructive'
							: 'block text-sm'
					}
				>
					{detail.last_test_message}
				</output>
			) : null}
			<div>
				<Button
					type="button"
					variant="outline"
					disabled={!canTest || testing}
					onClick={onTest}
				>
					{testing ? 'Test en cours…' : 'Tester la connexion'}
				</Button>
			</div>
		</section>
	)
}
