import { useForm, useStore } from '@tanstack/react-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { invalidateLlm } from '#/pages/llm/feature/invalidate'
import {
	buildCreateBody,
	buildUpdateBody,
	EMPTY_PROVIDER_VALUES,
	fieldErrorsFromPaths,
	numericErrors,
	valuesFromProvider,
} from '#/pages/llm/lib/provider-body'
import type {
	KeyMode,
	Provider,
	ProviderFieldName,
	ProviderFormValues,
} from '#/pages/llm/lib/types'
import { ProviderForm } from '#/pages/llm/ui/provider-form'

export function ProviderNewFeature() {
	return <ProviderFormFeature provider={null} />
}

export function ProviderEditFeature() {
	const { providerId } = useParams({ from: '/_app/llm/$providerId' })
	const navigate = useNavigate()
	const providerQuery = useQuery(
		lodestoneApi().get('/api/v1/llm/providers/{provider_id}', {
			path: { provider_id: providerId },
		}).queryOptions,
	)
	const provider = providerQuery.data?.data

	if (provider)
		return <ProviderFormFeature key={provider.id} provider={provider} />

	return (
		<div className="flex flex-col gap-3 p-4 md:p-8">
			{providerQuery.isError ? (
				<p role="alert" className="text-destructive">
					Impossible de charger le fournisseur :{' '}
					{errorMessage(providerQuery.error)}
				</p>
			) : (
				<p className="text-sm text-muted-foreground">
					Chargement du fournisseur…
				</p>
			)}
			<div>
				<button
					type="button"
					className="text-sm underline"
					onClick={() => void navigate({ to: '/llm' })}
				>
					Retour aux fournisseurs
				</button>
			</div>
		</div>
	)
}

interface ProviderFormFeatureProps {
	provider: Provider | null
}

function ProviderFormFeature({ provider }: ProviderFormFeatureProps) {
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const editing = provider !== null
	const initial = provider
		? valuesFromProvider(provider)
		: EMPTY_PROVIDER_VALUES
	const [keyMode, setKeyMode] = useState<KeyMode>('keep')
	const [localErrors, setLocalErrors] = useState<
		Partial<Record<ProviderFieldName, string>>
	>({})

	const done = async () => {
		await invalidateLlm(queryClient)
		void navigate({ to: '/llm' })
	}

	const create = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/llm/providers').mutationOptions,
		gcTime: 0,
		onSuccess: done,
	})
	const update = useMutation({
		...lodestoneApi().mutation('patch', '/api/v1/llm/providers/{provider_id}')
			.mutationOptions,
		gcTime: 0,
		onSuccess: done,
	})
	const active = editing ? update : create

	const form = useForm({
		defaultValues: initial,
		onSubmit: ({ value }) => {
			const errors = numericErrors(value)
			setLocalErrors(errors)
			if (Object.keys(errors).length > 0) return
			if (provider) {
				update.mutate({
					path: { provider_id: provider.id },
					body: buildUpdateBody(value, keyMode),
				})
			} else {
				create.mutate({ body: buildCreateBody(value) })
			}
		},
	})

	const values = useStore(form.store, (state) => state.values)
	const parsed = active.isError ? parseApiError(active.error) : null
	const serverErrors = parsed ? fieldErrorsFromPaths(parsed.fields) : {}
	const fieldErrors = { ...serverErrors, ...localErrors }
	const formError =
		parsed && Object.keys(parsed.fields).length === 0 ? parsed.message : null
	const dirty = !editing || keyMode !== 'keep' || !sameValues(values, initial)

	return (
		<ProviderForm
			title={provider ? `Modifier ${provider.name}` : 'Nouveau fournisseur'}
			editing={editing}
			hasApiKey={provider?.has_api_key ?? false}
			values={values}
			fieldErrors={fieldErrors}
			formError={formError}
			keyMode={keyMode}
			saving={active.isPending}
			dirty={dirty}
			onChange={(field, value) => {
				if (active.isError) active.reset()
				setLocalErrors({})
				form.setFieldValue(field, value)
			}}
			onKeyModeChange={(mode) => {
				setKeyMode(mode)
				form.setFieldValue('api_key', '')
			}}
			onSave={() => void form.handleSubmit()}
			onBack={() => void navigate({ to: '/llm' })}
		/>
	)
}

function sameValues(a: ProviderFormValues, b: ProviderFormValues): boolean {
	return (
		a.name === b.name &&
		a.kind === b.kind &&
		a.base_url === b.base_url &&
		a.model === b.model &&
		a.max_retries === b.max_retries &&
		a.timeout_seconds === b.timeout_seconds &&
		a.input_cents_per_mtok === b.input_cents_per_mtok &&
		a.output_cents_per_mtok === b.output_cents_per_mtok
	)
}
