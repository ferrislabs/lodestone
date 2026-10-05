import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { keyLabel } from '#/pages/llm/lib/tiers'
import type { Provider, TestResult } from '#/pages/llm/lib/types'

const SKELETON_ROWS = ['a', 'b', 'c']
const COLUMNS = 8

export interface TestOutcome {
	providerId: string
	providerName: string
	result: TestResult | null
	error: string | null
}

interface ProvidersTableProps {
	providers: readonly Provider[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	actionError: string | null
	busyId: string | null
	testingId: string | null
	confirmingDeleteId: string | null
	testOutcome: TestOutcome | null
	onEdit: (providerId: string) => void
	onToggleEnabled: (provider: Provider) => void
	onTest: (providerId: string) => void
	onRequestDelete: (providerId: string) => void
	onConfirmDelete: (providerId: string) => void
	onCancelDelete: () => void
}

export function ProvidersTable({
	providers,
	state,
	errorMessage,
	actionError,
	busyId,
	testingId,
	confirmingDeleteId,
	testOutcome,
	onEdit,
	onToggleEnabled,
	onTest,
	onRequestDelete,
	onConfirmDelete,
	onCancelDelete,
}: ProvidersTableProps) {
	return (
		<div className="flex flex-col gap-3">
			{actionError ? (
				<p role="alert" className="text-sm text-destructive">
					{actionError}
				</p>
			) : null}
			{testOutcome ? <TestOutcomeView outcome={testOutcome} /> : null}
			<div className="rounded-xl border bg-card">
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Name</TableHead>
							<TableHead>Kind</TableHead>
							<TableHead>Model</TableHead>
							<TableHead>Base URL</TableHead>
							<TableHead>Status</TableHead>
							<TableHead>API key</TableHead>
							<TableHead>Tiers</TableHead>
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
									Could not load providers: {errorMessage}
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready' && providers.length === 0 ? (
							<TableRow>
								<TableCell
									colSpan={COLUMNS}
									className="py-10 text-center text-muted-foreground"
								>
									No providers yet.
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready'
							? providers.map((provider) => (
									<TableRow key={provider.id}>
										<TableCell>
											<button
												type="button"
												className="font-medium text-foreground hover:underline"
												onClick={() => onEdit(provider.id)}
											>
												{provider.name}
											</button>
										</TableCell>
										<TableCell>{provider.kind}</TableCell>
										<TableCell>{provider.model}</TableCell>
										<TableCell className="max-w-48 truncate text-muted-foreground">
											{provider.base_url}
										</TableCell>
										<TableCell>
											<Badge
												variant={provider.enabled ? 'default' : 'secondary'}
											>
												{provider.enabled ? 'Enabled' : 'Disabled'}
											</Badge>
										</TableCell>
										<TableCell>
											<Badge
												variant={provider.has_api_key ? 'outline' : 'secondary'}
											>
												{keyLabel(provider.has_api_key)}
											</Badge>
										</TableCell>
										<TableCell>
											<div className="flex flex-wrap gap-1">
												{provider.tiers.map((tier) => (
													<Badge key={tier} variant="secondary">
														{tier}
													</Badge>
												))}
											</div>
										</TableCell>
										<TableCell>
											<div className="flex flex-wrap items-center justify-end gap-2">
												{confirmingDeleteId === provider.id ? (
													<>
														<Button
															size="sm"
															variant="destructive"
															disabled={busyId === provider.id}
															onClick={() => onConfirmDelete(provider.id)}
														>
															Confirm delete
														</Button>
														<Button
															size="sm"
															variant="outline"
															onClick={onCancelDelete}
														>
															Cancel
														</Button>
													</>
												) : (
													<>
														<Button
															size="sm"
															variant="outline"
															disabled={testingId === provider.id}
															onClick={() => onTest(provider.id)}
														>
															{testingId === provider.id
																? 'Testing...'
																: 'Test connection'}
														</Button>
														<Button
															size="sm"
															variant="outline"
															disabled={busyId === provider.id}
															onClick={() => onToggleEnabled(provider)}
														>
															{provider.enabled ? 'Disable' : 'Enable'}
														</Button>
														<Button
															size="sm"
															variant="outline"
															onClick={() => onRequestDelete(provider.id)}
														>
															Delete
														</Button>
													</>
												)}
											</div>
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

function TestOutcomeView({ outcome }: { outcome: TestOutcome }) {
	if (outcome.error) {
		return (
			<p role="alert" className="text-sm text-destructive">
				Test of {outcome.providerName} failed: {outcome.error}
			</p>
		)
	}
	const result = outcome.result
	if (result?.ok) {
		return (
			<output className="block text-sm">
				{outcome.providerName}: connection ok, model {result.model}, latency{' '}
				{result.latency_ms} ms
			</output>
		)
	}
	return (
		<p role="alert" className="text-sm text-destructive">
			{outcome.providerName}: {result?.error?.kind ?? 'error'} -{' '}
			{result?.error?.message ?? 'the test failed'}
		</p>
	)
}
