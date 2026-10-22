import { createFileRoute } from '@tanstack/react-router'

import { DroppedFeature } from '#/pages/quality/feature/dropped-feature'
import { droppedSearchSchema } from '#/pages/quality/lib/dropped'

export const Route = createFileRoute('/_app/quality/dropped')({
	validateSearch: droppedSearchSchema,
	head: () => ({ meta: [{ title: 'Annonces écartées · Lodestone' }] }),
	component: DroppedFeature,
})
