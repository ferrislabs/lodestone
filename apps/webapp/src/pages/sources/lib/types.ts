import type { Schemas } from '#/api/api.client'

export type Source = Schemas.SourceDto
export type SourceRun = Schemas.SourceRunDto
export type Listener = Schemas.ListenerDto

export interface ListenerSummary {
	state: string
	last_activity_at?: string | null
}

export type SourceState =
	| 'pending'
	| 'running'
	| 'failed'
	| 'succeeded'
	| 'cancelled'
	| 'unavailable'

export type TriggerResult =
	| { kind: 'scheduled' }
	| { kind: 'running' }
	| { kind: 'error'; message: string }
