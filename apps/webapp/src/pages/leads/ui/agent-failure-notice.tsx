import { Button } from '#/components/ui/button'
import { retryStatusLabel } from '#/pages/leads/lib/agent-failure'
import type { AgentFailure } from '#/pages/leads/lib/types'

interface AgentFailureNoticeProps {
	failure: AgentFailure
	onRetry: () => void
	retrying: boolean
	retryError: string | null
}

export function AgentFailureNotice({
	failure,
	onRetry,
	retrying,
	retryError,
}: AgentFailureNoticeProps) {
	return (
		<article
			data-agent-failure={failure.kind}
			data-failed={failure.failed ? 'true' : 'false'}
			className="flex flex-col gap-2 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-800"
		>
			<p className="font-medium">
				Agent « {failure.agent_name} » : appel au modèle en échec
			</p>
			<p data-part="failure-error">{failure.error}</p>
			<p data-part="failure-status">{retryStatusLabel(failure)}</p>
			{failure.failed ? (
				<div>
					<Button
						variant="outline"
						size="sm"
						disabled={retrying}
						onClick={onRetry}
					>
						Réessayer
					</Button>
				</div>
			) : null}
			{retryError ? (
				<p role="alert" className="text-destructive">
					{retryError}
				</p>
			) : null}
		</article>
	)
}
