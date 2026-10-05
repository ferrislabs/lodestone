import type { Schemas } from '#/api/api.client'

export type Source = Schemas.SourceDto
export type SourceRun = Schemas.SourceRunDto

export type SourceState =
	| 'pending'
	| 'running'
	| 'failed'
	| 'succeeded'
	| 'unavailable'

export type TriggerResult =
	| { kind: 'scheduled' }
	| { kind: 'running' }
	| { kind: 'error'; message: string }
