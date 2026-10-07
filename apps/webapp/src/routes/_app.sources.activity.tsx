import { createFileRoute } from '@tanstack/react-router'

import { SourcesActivityFeature } from '#/pages/source-providers/feature/activity-feature'

export const Route = createFileRoute('/_app/sources/activity')({
	head: () => ({ meta: [{ title: 'Activité des sources · Lodestone' }] }),
	component: SourcesActivityFeature,
})
