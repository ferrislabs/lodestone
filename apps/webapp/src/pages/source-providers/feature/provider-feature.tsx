import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useState } from 'react'

import { errorMessage, lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { invalidateSourceProviders } from '#/pages/source-providers/feature/invalidate'
import {
	buildBody,
	fieldErrors,
	initialValues,
	isDirty,
} from '#/pages/source-providers/lib/form'
import { callbackErrorMessage } from '#/pages/source-providers/lib/labels'
import type {
	FieldValues,
	ProviderDetail,
	SecretInputs,
} from '#/pages/source-providers/lib/types'
import {
	type Notice,
	ProviderPage,
} from '#/pages/source-providers/ui/provider-page'

const ROUTE = '/_app/sources/providers/$key'

export function ProviderFeature() {
	const { key } = useParams({ from: ROUTE })
	const search = useSearch({ from: ROUTE })
	const navigate = useNavigate()
	const [version, setVersion] = useState(0)
	const query = useQuery(
		lodestoneApi().get('/api/v1/source-providers/{key}', {
			path: { key },
		}).queryOptions,
	)
	const detail = query.data?.data

	if (detail) {
		return (
			<ProviderEditor
				key={`${key}:${version}`}
				detail={detail}
				landing={landingNotice(search)}
				onRefreshed={() => setVersion((current) => current + 1)}
			/>
		)
	}

	return (
		<div className="flex flex-col gap-3 p-4 md:p-8">
			{query.isError ? (
				<p role="alert" className="text-destructive">
					Impossible de charger la source : {errorMessage(query.error)}
				</p>
			) : (
				<p className="text-sm text-muted-foreground">
					Chargement de la source…
				</p>
			)}
			<div>
				<button
					type="button"
					className="text-sm underline"
					onClick={() => void navigate({ to: '/sources' })}
				>
					Retour au catalogue
				</button>
			</div>
		</div>
	)
}

interface LandingSearch {
	connected?: boolean
	error?: string
}

function landingNotice(search: LandingSearch): Notice | null {
	if (search.error) {
		return { kind: 'error', text: callbackErrorMessage(search.error) }
	}
	if (search.connected) {
		return {
			kind: 'success',
			text: 'Compte connecté. La connexion a été testée.',
		}
	}
	return null
}

interface ProviderEditorProps {
	detail: ProviderDetail
	landing: Notice | null
	onRefreshed: () => void
}

function ProviderEditor({ detail, landing, onRefreshed }: ProviderEditorProps) {
	const navigate = useNavigate()
	const queryClient = useQueryClient()
	const key = detail.summary.key
	const [values, setValues] = useState<FieldValues>(() => initialValues(detail))
	const [secrets, setSecrets] = useState<SecretInputs>({})
	const [replacing, setReplacing] = useState<string[]>([])
	const [notice, setNotice] = useState<Notice | null>(landing)
	const [confirmingDelete, setConfirmingDelete] = useState(false)

	const refreshed = async () => {
		await invalidateSourceProviders(queryClient)
		onRefreshed()
	}

	const save = useMutation({
		...lodestoneApi().mutation('put', '/api/v1/source-providers/{key}')
			.mutationOptions,
		gcTime: 0,
		onSuccess: async () => {
			setNotice(null)
			await refreshed()
		},
	})
	const test = useMutation({
		...lodestoneApi().mutation('post', '/api/v1/source-providers/{key}/test')
			.mutationOptions,
		gcTime: 0,
		onSuccess: async () => {
			setNotice(null)
			await refreshed()
		},
		onError: (error) => setNotice({ kind: 'error', text: errorMessage(error) }),
	})
	const connect = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/source-providers/{key}/oauth/start',
		).mutationOptions,
		gcTime: 0,
		onSuccess: (response) => {
			window.location.assign(response.data.authorize_url)
		},
		onError: (error) => setNotice({ kind: 'error', text: errorMessage(error) }),
	})
	const disconnect = useMutation({
		...lodestoneApi().mutation(
			'post',
			'/api/v1/source-providers/{key}/disconnect',
		).mutationOptions,
		gcTime: 0,
		onSuccess: async () => {
			setNotice(null)
			await refreshed()
		},
		onError: (error) => setNotice({ kind: 'error', text: errorMessage(error) }),
	})
	const remove = useMutation({
		...lodestoneApi().mutation('delete', '/api/v1/source-providers/{key}')
			.mutationOptions,
		gcTime: 0,
		onSuccess: async () => {
			await invalidateSourceProviders(queryClient)
			void navigate({ to: '/sources' })
		},
		onError: (error) => {
			setConfirmingDelete(false)
			setNotice({ kind: 'error', text: errorMessage(error) })
		},
	})

	const parsed = save.isError ? parseApiError(save.error) : null
	const errors = parsed ? fieldErrors(parsed.fields) : {}
	const formError =
		parsed && Object.keys(parsed.fields).length === 0 ? parsed.message : null

	return (
		<ProviderPage
			detail={detail}
			values={values}
			secrets={secrets}
			replacing={replacing}
			errors={errors}
			formError={formError}
			notice={notice}
			dirty={isDirty(detail, values, secrets)}
			saving={save.isPending}
			testing={test.isPending}
			connecting={connect.isPending}
			disconnecting={disconnect.isPending}
			deleting={remove.isPending}
			confirmingDelete={confirmingDelete}
			onChange={(field, value) => {
				if (save.isError) save.reset()
				setValues((current) => ({ ...current, [field]: value }))
			}}
			onSecretChange={(field, value) => {
				if (save.isError) save.reset()
				setSecrets((current) => ({ ...current, [field]: value }))
			}}
			onReplace={(field) => setReplacing((current) => [...current, field])}
			onCancelReplace={(field) => {
				setReplacing((current) => current.filter((name) => name !== field))
				setSecrets((current) => ({ ...current, [field]: '' }))
			}}
			onSave={() =>
				save.mutate({ path: { key }, body: buildBody(detail, values, secrets) })
			}
			onTest={() => test.mutate({ path: { key } })}
			onConnect={() => connect.mutate({ path: { key } })}
			onDisconnect={() => disconnect.mutate({ path: { key } })}
			onRequestDelete={() => setConfirmingDelete(true)}
			onConfirmDelete={() => remove.mutate({ path: { key } })}
			onCancelDelete={() => setConfirmingDelete(false)}
			onBack={() => void navigate({ to: '/sources' })}
		/>
	)
}
