import { createFileRoute } from '@tanstack/react-router'

import { SourcesFeature } from '#/pages/sources/feature/sources-feature'

export const Route = createFileRoute('/_app/sources/')({
	head: () => ({ meta: [{ title: 'Sources · Lodestone' }] }),
	component: SourcesFeature,
})
