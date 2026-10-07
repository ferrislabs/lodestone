import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'

import { lodestoneApi } from '#/api/lodestone'
import { parseApiError } from '#/lib/api-errors'
import { invalidateSources } from '#/pages/sources/feature/invalidate'
import { RESULT_CLEAR_MS } from '#/pages/sources/lib/state'
import type { TriggerResult } from '#/pages/sources/lib/types'

const ALREADY_RUNNING = 'La source est déjà en cours'

export interface SourceTrigger {
	trigger: (source: string) => void
	results: Readonly<Record<string, TriggerResult>>
	triggeredAt: Readonly<Record<string, number>>
	pending: ReadonlySet<string>
}

export function useSourceTrigger(): SourceTrigger {
	const queryClient = useQueryClient()
	const [results, setResults] = useState<Record<string, TriggerResult>>({})
	const [triggeredAt, setTriggeredAt] = useState<Record<string, number>>({})
	const [pending, setPending] = useState<ReadonlySet<string>>(new Set())
	const timers = useRef<number[]>([])
	const mutation = useMutation(
		lodestoneApi().mutation('post', '/api/v1/sources/{source}/run')
			.mutationOptions,
	)

	useEffect(
		() => () => {
			for (const timer of timers.current) window.clearTimeout(timer)
		},
		[],
	)

	const settle = (source: string, result: TriggerResult) => {
		setResults((current) => ({ ...current, [source]: result }))
		setPending((current) => {
			const next = new Set(current)
			next.delete(source)
			return next
		})
		timers.current.push(
			window.setTimeout(() => {
				setResults((current) => {
					const { [source]: _cleared, ...rest } = current
					return rest
				})
			}, RESULT_CLEAR_MS),
		)
	}

	const run = async (source: string) => {
		try {
			await mutation.mutateAsync({ path: { source } })
			settle(source, { kind: 'scheduled' })
		} catch (error) {
			const parsed = parseApiError(error)
			setTriggeredAt((current) => {
				const { [source]: _cleared, ...rest } = current
				return rest
			})
			settle(
				source,
				parsed.status === 409 && parsed.message === ALREADY_RUNNING
					? { kind: 'running' }
					: { kind: 'error', message: parsed.message },
			)
		} finally {
			await invalidateSources(queryClient)
		}
	}

	const trigger = (source: string) => {
		setPending((current) => new Set(current).add(source))
		setResults((current) => {
			const { [source]: _cleared, ...rest } = current
			return rest
		})
		setTriggeredAt((current) => ({ ...current, [source]: Date.now() }))
		void run(source)
	}

	return { trigger, results, triggeredAt, pending }
}
